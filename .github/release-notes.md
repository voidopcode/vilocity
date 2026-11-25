Automatic build of each engine from the commit this release is tagged at, as two separate downloads. **GTA VI is not playable yet**: it boots, runs its menus at 30 fps and loads into the world, which renders slowly and loses the GPU device after a few minutes. See the README for the current measured status.

### Downloads

| File | Engine | For | Licence |
|------|--------|-----|---------|
| `vilocity-native-<version>-windows-x64.zip` | native | GTA VI | GPL-3.0-or-later |
| `vilocity-compat-<version>-windows-x64.zip` | compat | other Xbox Series X\|S (GDK) titles | GPL-2.0-only |

The two engines are under different licences and are shipped as separate programs on purpose. The combined `vilocity.exe` is not distributed; build it from source for your own use.

### Requirements

- Windows 10/11 x64, a CPU with AVX2
- A Direct3D 12 GPU with shader model 6.6 and mesh shaders
- DXC (`dxcompiler.dll`) from the Windows SDK or a DirectXShaderCompiler release (`VILOCITY_DXC_DIR`)
- Your own copy of the game. No game files are included.

### Use

```powershell
vilocity-native.exe run <game folder>\game_scarlett_master.exe --execute
vilocity-compat.exe run <game folder>
```

### Source

The source for both binaries is this release's tag. Vilocity is a modified version of GPL-licensed software, changed in November 2026.
