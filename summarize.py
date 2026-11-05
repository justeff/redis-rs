import json,sys,collections,statistics as st
f=sys.argv[1]; base=sys.argv[2] if len(sys.argv)>2 else 'base'
d=collections.defaultdict(list)
for l in open(f):
    l=l.strip()
    if not l.startswith('{'): continue
    r=json.loads(l); d[(r['workload'],r['tasks'],r['label'])].append(r)
keys=sorted(set((w,t) for w,t,_ in d))
labels=sorted(set(l for _,_,l in d), key=lambda x:(x!=base,x))
print(f"{'workload':<10}{'T':>5}  {'variant':<16}{'cpu us/op':>10}{'Δcpu':>8}{'kops/s':>9}{'Δthr':>8}{'n':>3}")
for w,t in keys:
    b=d.get((w,t,base))
    bc=st.median(x['cpu_us_per_op'] for x in b) if b else None
    bt=st.median(x['ops_per_s'] for x in b) if b else None
    for lab in labels:
        v=d.get((w,t,lab))
        if not v: continue
        c=st.median(x['cpu_us_per_op'] for x in v); th=st.median(x['ops_per_s'] for x in v)
        dc=f"{(c/bc-1)*100:+.1f}%" if bc and lab!=base else ''
        dt=f"{(th/bt-1)*100:+.1f}%" if bt and lab!=base else ''
        print(f"{w:<10}{t:>5}  {lab:<16}{c:>10.2f}{dc:>8}{th/1e3:>9.0f}{dt:>8}{len(v):>3}")
