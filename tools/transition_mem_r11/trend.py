"""Fixed-before-data E2 gate: every post-warm-up cycle, plus the last ten."""
WARMUP_CYCLES=3
RANGE_LIMIT_BYTES=16*1024
SLOPE_LIMIT_BYTES_PER_CYCLE=1024
def fit(values):
    n=len(values)
    mean_x=(n-1)/2
    mean_y=sum(values)/n
    return sum((x-mean_x)*(y-mean_y) for x,y in enumerate(values))/sum((x-mean_x)**2 for x in range(n))
def measure(values):
    # values[0] is startup. Drop exactly three operation samples, never an
    # adaptively chosen prefix that could hide a late capacity jump.
    later=values[1+WARMUP_CYCLES:]
    assert len(later)>=5, 'Too few post-warm-up samples'
    slope=fit(later);tail=fit(later[-10:]);span=max(later)-min(later)
    return dict(warmup_cycles=WARMUP_CYCLES,post_warmup_samples=len(later),
                slope_bytes_per_cycle=slope,last_ten_slope_bytes_per_cycle=tail,
                post_warmup_range_bytes=span,post_warmup_drift_bytes=later[-1]-later[0],
                range_limit_bytes=RANGE_LIMIT_BYTES,slope_limit_bytes_per_cycle=SLOPE_LIMIT_BYTES_PER_CYCLE,
                pass_trend=span<=RANGE_LIMIT_BYTES and abs(slope)<=SLOPE_LIMIT_BYTES_PER_CYCLE and abs(tail)<=SLOPE_LIMIT_BYTES_PER_CYCLE)

if __name__=='__main__':
    # A final flat pair cannot hide either the review's late 528 KiB step or
    # cumulative small growth. These examples passed the old last-delta gate.
    for sequence in [[0]+[1000000]*7+[1541104]*23,[0]+[i*8192 for i in range(30)]]:
        assert sequence[-1]-sequence[-2] <= 16384
        assert not measure(sequence)['pass_trend']
    assert measure([0]+[1000000]*30)['pass_trend']
    print('PASS late-step and cumulative-growth trend regressions')
