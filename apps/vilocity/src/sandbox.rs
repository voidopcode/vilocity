//! The sandbox `play` runs the native engine in. The title's x64 code runs in
//! the engine's own process, so the process itself is fenced in:
//!
//! * an AppContainer (`Vilocity.Sandbox`) with no capabilities: no network
//!   at all (loopback included), no access to the user's files, other
//!   processes' windows or memory, or anything a low integrity process cannot
//!   touch; it reads only what [`run`] grants and writes only the folders
//!   granted with `write`;
//! * a job object: every process of the title dies with `vilocity.exe`, and
//!   it can neither read nor write the clipboard, change system or display
//!   settings, use global atoms or log the user off.
//!
//! The GPU, audio and input go through the same drivers as outside, at the
//! same speed. Grants are added to the folders' access lists once and stay.

use std::ffi::{OsStr, OsString, c_void};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_SUCCESS, GetLastError, HANDLE, HANDLE_FLAG_INHERIT, LocalFree, SetHandleInformation,
    WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSidToSidW, EXPLICIT_ACCESS_W, GRANT_ACCESS, GetNamedSecurityInfoW, SE_FILE_OBJECT, SetEntriesInAclW, SetNamedSecurityInfoW,
    TRUSTEE_IS_SID, TRUSTEE_IS_WELL_KNOWN_GROUP, TRUSTEE_W,
};
use windows_sys::Win32::Security::Isolation::{CreateAppContainerProfile, DeriveAppContainerSidFromAppContainerName};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACL, ACL_SIZE_INFORMATION, AclSizeInformation, CONTAINER_INHERIT_ACE,
    DACL_SECURITY_INFORMATION, EqualSid, FreeSid, GetAce, GetAclInformation, NO_INHERITANCE, OBJECT_INHERIT_ACE,
    PSECURITY_DESCRIPTOR, PSID, SECURITY_CAPABILITIES, SUB_CONTAINERS_AND_OBJECTS_INHERIT,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_DELETE_CHILD, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
};
use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_UILIMIT_DESKTOP,
    JOB_OBJECT_UILIMIT_DISPLAYSETTINGS, JOB_OBJECT_UILIMIT_EXITWINDOWS, JOB_OBJECT_UILIMIT_GLOBALATOMS,
    JOB_OBJECT_UILIMIT_READCLIPBOARD, JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS, JOB_OBJECT_UILIMIT_WRITECLIPBOARD,
    JOBOBJECT_BASIC_UI_RESTRICTIONS, JobObjectBasicAccountingInformation, JobObjectBasicUIRestrictions,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, INFINITE, InitializeProcThreadAttributeList,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
    PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess,
    UpdateProcThreadAttribute, WaitForSingleObject,
};

const NAME: &str = "Vilocity.Sandbox";
const READ: u32 = FILE_GENERIC_READ | FILE_GENERIC_EXECUTE;
const WRITE: u32 = READ | FILE_GENERIC_WRITE | DELETE | FILE_DELETE_CHILD;

/// A path the sandbox may use: read (and run) it, or also write in it.
pub struct Grant {
    pub path: PathBuf,
    pub write: bool,
}

/// Runs `exe args` in the sandbox with `env` added to our environment and
/// `cwd` as its folder, after granting `grants`; returns its exit code once
/// every process of it has ended.
pub fn run(exe: &Path, args: &[OsString], grants: &[Grant], env: &[(&str, OsString)], cwd: &Path) -> Result<i32, String> {
    let sid = Sid::container()?;
    for g in grants.iter().chain([&Grant { path: exe.to_path_buf(), write: false }]) {
        grant(&g.path, sid.0, if g.write { WRITE } else { READ })
            .map_err(|e| format!("granting the sandbox {}: {e}", g.path.display()))?;
    }
    let job = Job::new()?;
    // SAFETY: plain Win32 calls on handles and buffers that outlive them.
    unsafe {
        let mut caps: SECURITY_CAPABILITIES = std::mem::zeroed();
        caps.AppContainerSid = sid.0;
        // Only our standard handles pass to the title, nothing else we hold.
        let std = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE].map(|n| GetStdHandle(n));
        let mut handles: Vec<HANDLE> = std
            .into_iter()
            .filter(|&h| !h.is_null() && h as isize != -1 && SetHandleInformation(h, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) != 0)
            .collect();
        // stdout and stderr are often one pipe: listed once.
        handles.sort();
        handles.dedup();
        let mut size = 0usize;
        InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut size);
        let mut buf = vec![0u8; size];
        let list = buf.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
        if InitializeProcThreadAttributeList(list, 2, 0, &mut size) == 0 {
            return Err(last("InitializeProcThreadAttributeList"));
        }
        let attrs = Attrs(list);
        let set = |attr: usize, value: *const c_void, len: usize| {
            UpdateProcThreadAttribute(attrs.0, 0, attr, value, len, null_mut(), null()) != 0
        };
        if !set(PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize, (&raw const caps).cast(), size_of_val(&caps))
            || (!handles.is_empty()
                && !set(PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize, handles.as_ptr().cast(), size_of_val(&handles[..])))
        {
            return Err(last("UpdateProcThreadAttribute"));
        }
        let mut si: STARTUPINFOEXW = std::mem::zeroed();
        si.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        si.lpAttributeList = list;
        if !handles.is_empty() {
            // Those that are not passed on stay null in the title.
            let pass = |h: HANDLE| if handles.contains(&h) { h } else { null_mut() };
            si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            si.StartupInfo.hStdInput = pass(std[0]);
            si.StartupInfo.hStdOutput = pass(std[1]);
            si.StartupInfo.hStdError = pass(std[2]);
        }
        let mut cmd = wide(&command_line(exe.as_os_str(), args));
        let mut block = environment(env);
        let (app, dir) = (wide(exe.as_os_str()), wide(cwd.as_os_str()));
        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
        // Suspended until it is in the job, so no instruction of it runs outside.
        if CreateProcessW(
            app.as_ptr(),
            cmd.as_mut_ptr(),
            null(),
            null(),
            i32::from(!handles.is_empty()),
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_SUSPENDED,
            block.as_mut_ptr().cast(),
            dir.as_ptr(),
            &si.StartupInfo,
            &mut pi,
        ) == 0
        {
            return Err(last("starting the sandboxed engine"));
        }
        if AssignProcessToJobObject(job.0, pi.hProcess) == 0 {
            let e = last("AssignProcessToJobObject");
            TerminateProcess(pi.hProcess, 1);
            CloseHandle(pi.hThread);
            CloseHandle(pi.hProcess);
            return Err(e);
        }
        ResumeThread(pi.hThread);
        CloseHandle(pi.hThread);
        // The cursor clip the sandbox may not do itself.
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let clip = {
            let (pid, done) = (pi.dwProcessId, done.clone());
            std::thread::spawn(move || crate::mouseclip::watch(pid, done))
        };
        let mut code = 1u32;
        if WaitForSingleObject(pi.hProcess, INFINITE) == WAIT_OBJECT_0 {
            GetExitCodeProcess(pi.hProcess, &mut code);
        }
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = clip.join();
        CloseHandle(pi.hProcess);
        // A title that relaunched itself goes on in a new process of the job.
        // shortcut: polls twice a second; a completion port would wake at once.
        while job.active() > 0 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        Ok(code as i32)
    }
}

struct Attrs(LPPROC_THREAD_ATTRIBUTE_LIST);
impl Drop for Attrs {
    fn drop(&mut self) {
        // SAFETY: initialised by InitializeProcThreadAttributeList.
        unsafe { DeleteProcThreadAttributeList(self.0) };
    }
}

/// The AppContainer's SID, the profile made on first use.
struct Sid(PSID);
impl Sid {
    fn container() -> Result<Self, String> {
        let name = wide(OsStr::new(NAME));
        let display = wide(OsStr::new("Vilocity sandbox"));
        let mut sid: PSID = null_mut();
        // SAFETY: out-pointer to a SID the call allocates.
        let hr = unsafe { CreateAppContainerProfile(name.as_ptr(), display.as_ptr(), display.as_ptr(), null(), 0, &mut sid) };
        if hr >= 0 {
            return Ok(Sid(sid));
        }
        // SAFETY: as above; the profile exists from an earlier run.
        let hr = unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut sid) };
        if hr < 0 {
            return Err(format!("no AppContainer profile (HRESULT {hr:#010x})"));
        }
        Ok(Sid(sid))
    }
}
impl Drop for Sid {
    fn drop(&mut self) {
        // SAFETY: allocated by the AppContainer API above.
        unsafe { FreeSid(self.0) };
    }
}

struct Job(HANDLE);
impl Job {
    fn new() -> Result<Self, String> {
        // SAFETY: plain job object set-up with structures on the stack.
        unsafe {
            let job = Job(CreateJobObjectW(null(), null()));
            if job.0.is_null() {
                return Err(last("CreateJobObjectW"));
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ui = JOBOBJECT_BASIC_UI_RESTRICTIONS {
                UIRestrictionsClass: JOB_OBJECT_UILIMIT_READCLIPBOARD
                    | JOB_OBJECT_UILIMIT_WRITECLIPBOARD
                    | JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS
                    | JOB_OBJECT_UILIMIT_DISPLAYSETTINGS
                    | JOB_OBJECT_UILIMIT_GLOBALATOMS
                    | JOB_OBJECT_UILIMIT_DESKTOP
                    | JOB_OBJECT_UILIMIT_EXITWINDOWS,
            };
            if SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size_of_val(&limits) as u32,
            ) == 0
                || SetInformationJobObject(job.0, JobObjectBasicUIRestrictions, (&raw const ui).cast(), size_of_val(&ui) as u32)
                    == 0
            {
                return Err(last("SetInformationJobObject"));
            }
            Ok(job)
        }
    }

    fn active(&self) -> u32 {
        // SAFETY: query into a structure on the stack.
        unsafe {
            let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
            let ok = QueryInformationJobObject(
                self.0,
                JobObjectBasicAccountingInformation,
                (&raw mut info).cast(),
                size_of_val(&info) as u32,
                null_mut(),
            );
            if ok == 0 { 0 } else { info.ActiveProcesses }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: our handle; closing it kills what is left in the job.
        unsafe { CloseHandle(self.0) };
    }
}

/// Adds an allow entry for `sid` with `mask` to `path`'s access list (a
/// folder's entry is inherited by everything in it), unless one is there.
fn grant(path: &Path, sid: PSID, mask: u32) -> Result<(), String> {
    let name = wide(path.as_os_str());
    // SAFETY: the security descriptor is freed below; the ACL points into it.
    unsafe {
        let mut dacl: *mut ACL = null_mut();
        let mut sd: PSECURITY_DESCRIPTOR = null_mut();
        let rc = GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut sd,
        );
        if rc != ERROR_SUCCESS {
            return Err(format!("reading its access list: error {rc}"));
        }
        let _free = Local(sd);
        // Every AppContainer is in ALL APPLICATION PACKAGES (system folders allow it).
        let mut any: PSID = null_mut();
        let any_name = wide(OsStr::new("S-1-15-2-1"));
        if ConvertStringSidToSidW(any_name.as_ptr(), &mut any) == 0 {
            return Err(last("ConvertStringSidToSidW"));
        }
        let _free_any = Local(any);
        if has(dacl, sid, mask, path.is_dir()) || has(dacl, any, mask, path.is_dir()) {
            return Ok(());
        }
        let ea = EXPLICIT_ACCESS_W {
            grfAccessPermissions: mask,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: if path.is_dir() { SUB_CONTAINERS_AND_OBJECTS_INHERIT } else { NO_INHERITANCE },
            Trustee: TRUSTEE_W {
                pMultipleTrustee: null_mut(),
                MultipleTrusteeOperation: 0,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
                ptstrName: sid.cast(),
            },
        };
        let mut new: *mut ACL = null_mut();
        let rc = SetEntriesInAclW(1, &ea, dacl, &mut new);
        if rc != ERROR_SUCCESS {
            return Err(format!("building its access list: error {rc}"));
        }
        let _free_new = Local(new.cast());
        let rc = SetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            new,
            null(),
        );
        if rc != ERROR_SUCCESS {
            return Err(format!("writing its access list: error {rc}"));
        }
        Ok(())
    }
}

/// Whether `dacl` already allows `sid` everything in `mask`, passed on to
/// what is inside when `inherit` (folders).
unsafe fn has(dacl: *const ACL, sid: PSID, mask: u32, inherit: bool) -> bool {
    if dacl.is_null() {
        return false;
    }
    let onward = OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE;
    // SAFETY: walks the ACEs of a valid ACL.
    unsafe {
        let mut info: ACL_SIZE_INFORMATION = std::mem::zeroed();
        if GetAclInformation(dacl, (&raw mut info).cast(), size_of_val(&info) as u32, AclSizeInformation) == 0 {
            return false;
        }
        (0..info.AceCount).any(|i| {
            let mut ace: *mut c_void = null_mut();
            if GetAce(dacl, i, &mut ace) == 0 {
                return false;
            }
            let ace = &*(ace as *const ACCESS_ALLOWED_ACE);
            // ACCESS_ALLOWED_ACE_TYPE is 0.
            ace.Header.AceType == 0
                && (!inherit || u32::from(ace.Header.AceFlags) & onward == onward)
                && ace.Mask & mask == mask
                && EqualSid((&raw const ace.SidStart).cast_mut().cast(), sid) != 0
        })
    }
}

struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        // SAFETY: allocated with LocalAlloc by the security API.
        unsafe { LocalFree(self.0) };
    }
}

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain([0]).collect()
}

fn last(what: &str) -> String {
    // SAFETY: reads the thread's last error.
    format!("{what}: error {}", unsafe { GetLastError() })
}

/// Our environment with `extra` set, as a CreateProcessW block.
fn environment(extra: &[(&str, OsString)]) -> Vec<u16> {
    let mut vars: Vec<(OsString, OsString)> = std::env::vars_os()
        .filter(|(k, _)| !extra.iter().any(|(e, _)| OsStr::new(e).eq_ignore_ascii_case(k)))
        .collect();
    vars.extend(extra.iter().map(|(k, v)| (OsString::from(k), v.clone())));
    let mut block = Vec::new();
    for (k, v) in vars {
        block.extend(k.encode_wide());
        block.push(u16::from(b'='));
        block.extend(v.encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

/// `exe args` quoted the way the C runtime splits a command line.
fn command_line(exe: &OsStr, args: &[OsString]) -> OsString {
    let mut out = OsString::new();
    for (i, a) in std::iter::once(exe).chain(args.iter().map(OsString::as_os_str)).enumerate() {
        if i > 0 {
            out.push(" ");
        }
        out.push(quote(&a.to_string_lossy()));
    }
    out
}

fn quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_owned();
    }
    let mut out = String::from('"');
    let mut slashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => slashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(slashes * 2 + 1));
                out.push('"');
                slashes = 0;
                continue;
            }
            _ => {}
        }
        if c != '\\' {
            out.push_str(&"\\".repeat(slashes));
            slashes = 0;
            out.push(c);
        }
    }
    out.push_str(&"\\".repeat(slashes * 2));
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_quoted_like_the_c_runtime_splits_them() {
        assert_eq!(quote("plain"), "plain");
        assert_eq!(quote(r"D:\Game Folder\game.exe"), r#""D:\Game Folder\game.exe""#);
        assert_eq!(quote(r"ends\"), r"ends\");
        assert_eq!(quote(r"a b\"), r#""a b\\""#);
        assert_eq!(quote(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(quote(""), r#""""#);
    }

    /// The fence holds: a program in the sandbox cannot read or plant a file
    /// outside what it was given, writes in its granted folder, and opens no
    /// socket, not even on loopback.
    #[test]
    fn the_sandbox_reaches_only_what_it_is_granted() {
        let root = std::env::temp_dir().join(format!("vilocity-sandbox-test-{}", std::process::id()));
        let (secret, open) = (root.join("secret"), root.join("granted"));
        std::fs::create_dir_all(&secret).unwrap();
        std::fs::create_dir_all(&open).unwrap();
        std::fs::write(secret.join("passwords.txt"), "x").unwrap();
        let sys = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32");
        let grants = [Grant { path: open.clone(), write: true }];
        let sandboxed = |exe: &str, args: &[String]| {
            let args: Vec<OsString> = args.iter().map(OsString::from).collect();
            run(&sys.join(exe), &args, &grants, &[], &open).unwrap()
        };
        let cmd = |line: String| sandboxed("cmd.exe", &["/c".into(), line]);

        assert_ne!(cmd(format!("type {}", secret.join("passwords.txt").display())), 0, "read a file it was not given");
        cmd(format!("echo x> {}", secret.join("planted.txt").display()));
        assert!(!secret.join("planted.txt").exists(), "wrote outside its folder");
        assert_eq!(cmd(format!("echo x> {}", open.join("ok.txt").display())), 0);
        assert!(open.join("ok.txt").exists(), "could not write in its own folder");

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}/", listener.local_addr().unwrap().port());
        sandboxed("curl.exe", &["-s".into(), "-m".into(), "3".into(), url]);
        listener.set_nonblocking(true).unwrap();
        assert!(listener.accept().is_err(), "connected to a socket");
        let _ = std::fs::remove_dir_all(&root);
    }
}

