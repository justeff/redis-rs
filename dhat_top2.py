import json,sys,re
d=json.load(open(sys.argv[1])); ops=float(sys.argv[2]); n=int(sys.argv[3]) if len(sys.argv)>3 else 12
ft=d['ftbl']
def short(f):
    f=re.sub(r'^0x[0-9a-f]+: ','',f)
    m=re.search(r'\((\S+\.rs):(\d+)',f); loc=(m.group(1).split('/')[-1]+':'+m.group(2)) if m else ''
    f=re.sub(r' \(.*\)$','',f)
    for _ in range(4): f=re.sub(r'<[^<>]*>','',f)
    return (f[:70]+'@'+loc)
pps=sorted(d['pps'], key=lambda p:-p['tbk'])
for p in pps[:n]:
    frames=[short(ft[i]) for i in p['fs']]
    interesting=[f for f in frames if ('redis' in f or 'parser.rs' in f or 'cmd.rs' in f or 'combine' in f or 'rrbench' in f or 'main.rs' in f)][:3]
    print(f"{p['tbk']/ops:7.2f}/op {p['tb']/p['tbk']:6.0f}B  "+' <- '.join(interesting))
