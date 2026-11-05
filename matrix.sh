#!/bin/zsh
# matrix.sh <outfile> <reps> <variants...>   (WORKLOADS / TASKS env override)
cd "$(dirname "$0")"
OUT=$1; REPS=$2; shift 2; VARIANTS=($@)
WORKLOADS=(${=WORKLOADS:-get set mget100 hgetall pipe100})
TASKS=(${=TASKS:-16 128})
ops() { case $1 in get|set|getdirect|incr|setkey) echo ${OPS_SMALL:-300000};; get1k) echo 200000;; mget100|hgetall) echo ${OPS_AGG:-60000};; pipe100) echo 30000;; esac }
: > $OUT
for rep in $(seq 1 $REPS); do
  for w in $WORKLOADS; do for t in $TASKS; do
    # rotate variant order per rep to cancel drift
    vs=($VARIANTS); n=${#vs}; k=$(( (rep-1) % n ))
    ordered=(${vs[$((k+1)),-1]} ${vs[1,$k]})
    for v in $ordered; do ./run.sh $v --workload $w --tasks $t --ops $(ops $w) $EXTRA >> $OUT; done
  done; done
done
