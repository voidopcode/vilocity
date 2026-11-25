"""Resolves diff3 conflict blocks in a file. resolve(path, {n: rule}); rule: 'o','f','of','fo' or fn(o,b,f)->str."""
import re
def resolve(path, rules):
    s = open(path, encoding="utf-8", newline="").read()
    pat = re.compile(r"<<<<<<< ours\n(.*?)\|\|\|\|\|\|\| base\n(.*?)=======\n(.*?)>>>>>>> fork\n", re.S)
    n = 0
    def rep(m):
        nonlocal n
        n += 1
        o, b, f = m.groups()
        r = rules[n]
        if callable(r):
            return r(o, b, f)
        return {"o": o, "f": f, "of": o + f, "fo": f + o}[r]
    s = pat.sub(rep, s)
    assert n == len(rules), (path, n, len(rules))
    open(path, "w", encoding="utf-8", newline="").write(s)
