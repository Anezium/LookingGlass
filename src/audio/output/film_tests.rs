//! Actual Mixer -> Monitor/limiter -> SPSC -> Callback regression. No device,
//! synthetic prefill, scheduler luck, or host performance assertion.
use super::*;
use crate::audio::{Monitor, MonitorData};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};

const GUARD: Duration = Duration::from_secs(10);
fn other(i: usize) -> f32 {
    if i % 2 == 0 { 0.125 + (i / 2) as f32 / 65536. }
    else { -0.25 - (i / 2) as f32 / 131072. }
}
fn film(i: usize) -> f32 { if i % 2 == 0 { 0.3 } else { 0.4 } }
fn new_film(i: usize) -> f32 { if i % 2 == 0 { 0.45 + (i / 2) as f32 / 262144. } else { 0.35 - (i / 2) as f32 / 262144. } }
fn limited(v: f32) -> f32 { crate::audio::acoustics::limit(v) }
struct Gate { at: usize, entered: SyncSender<()>, release: Receiver<()> }
struct Indexed {
    index: usize,
    value: fn(usize) -> f32,
    calls: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
    gate: Option<Gate>,
}
impl Iterator for Indexed {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.gate.as_ref().is_some_and(|g| g.at == self.index) {
            let gate = self.gate.take().unwrap();
            gate.entered.send(()).unwrap();
            gate.release.recv_timeout(GUARD).unwrap();
        }
        let value = (self.value)(self.index);
        self.index += 1;
        self.calls.fetch_add(1, Ordering::Relaxed);
        Some(value)
    }
}
impl Drop for Indexed { fn drop(&mut self) { self.drops.fetch_add(1, Ordering::Relaxed); } }
impl Source for Indexed {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { 2 }
    fn sample_rate(&self) -> u32 { 44100 }
    fn total_duration(&self) -> Option<Duration> { None }
}
fn append(sink: &Sink, value: fn(usize)->f32, gate: Option<Gate>) -> (Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    sink.append(Indexed { index: 0, value, calls: calls.clone(), drops: drops.clone(), gate });
    (calls, drops)
}
fn monitor(mixer: Mixer) -> Monitor<Mixer> {
    Monitor { source: mixer, data: Arc::new(MonitorData::default()), block: Vec::with_capacity(SAMPLES), record: false }
}
fn callback(rx: queue::Receiver<Block>, metrics: Arc<Metrics>) -> Callback {
    Callback { ready: rx, block: [0.;SAMPLES], without_movie: [0.;SAMPLES], movie: 0,
        use_movie: true, at: SAMPLES, epoch: 0, metrics }
}
fn expect(callback: &mut Callback, start: usize, end: usize, normal_until: usize) {
    let scope = crate::kernel_alloc::Scope::begin();
    for i in start..end {
        let expected = limited(other(i) + if i < normal_until { film(i) } else { 0. });
        assert_eq!(callback.next().unwrap().to_bits(), expected.to_bits(), "PCM index {i}; raw={}", callback.metrics.starved.load(Ordering::Relaxed));
    }
    assert_eq!(scope.finish(), 0, "callback allocated");
}

#[test]
fn full_real_cushion_selectively_removes_film_preserving_every_pcm_index() {
    for mix in [true, false] {
        for (capacity, cushion) in [(1,1), (4,4), (8,8), (8,1), (8,4)] {
            for prefix in [0, 1, 2, 137, SAMPLES] {
                let (control, mut mixer) = mixer();
                mixer.config.mix = mix;
                let continuous = control.sink();
                let (calls, drops) = append(&continuous, other, None);
                let movie = control.movie_sink();
                let (film_calls, film_drops) = append(&movie, film, None);
                let (tx, rx) = queue::channel(capacity);
                let mut callback = callback(rx, control.metrics.clone());
                let (entered, seen) = sync_channel(0);
                let (release, blocked) = sync_channel(0);
                let metrics = control.metrics.clone();
                let worker = thread::spawn(move || {
                    let mut monitor = monitor(mixer);
                    for _ in 0..cushion { assert!(producer_step(&mut monitor, &tx, &metrics, cushion, |_| {}).is_some()); }
                    assert!(producer_step(&mut monitor, &tx, &metrics, cushion, |p| {
                        assert_eq!(p, ProducerPoint::Full);
                        entered.send(()).unwrap(); blocked.recv_timeout(GUARD).unwrap();
                    }).is_none());
                    (monitor, tx)
                });
                seen.recv_timeout(GUARD).unwrap();
                assert_eq!(control.metrics.queued_samples.load(Ordering::Relaxed), cushion*SAMPLES);
                expect(&mut callback, 0, prefix, usize::MAX);
                let counts = (calls.load(Ordering::Relaxed), film_calls.load(Ordering::Relaxed));
                movie.stop(); // The same public stop used by movie quit/skip/end.
                let frontier = (prefix + 1) & !1;
                expect(&mut callback, prefix, cushion*SAMPLES, frontier);
                assert_eq!(control.metrics.starved.load(Ordering::Relaxed), 0);
                assert_eq!(control.metrics.flushes.load(Ordering::Relaxed), 0);
                assert_eq!(control.metrics.queued_samples.load(Ordering::Relaxed), 0);
                assert_eq!(counts, (calls.load(Ordering::Relaxed), film_calls.load(Ordering::Relaxed)));
                assert_eq!(drops.load(Ordering::Relaxed) + film_drops.load(Ordering::Relaxed), 0);
                // Nothing remains: the exact same raw predicate MUST now fire.
                let scope = crate::kernel_alloc::Scope::begin();
                for _ in 0..SAMPLES { assert_eq!(callback.next(), Some(0.)); }
                assert_eq!(scope.finish(), 0);
                assert_eq!(control.metrics.starved.load(Ordering::Relaxed), 1);
                assert_eq!(control.metrics.flush_silence.load(Ordering::Relaxed), 0);
                release.send(()).unwrap();
                let _producer = worker.join().unwrap();
            }
        }
    }
}

#[test]
fn in_flight_and_rapid_rearm_never_admit_the_old_movie_or_retag_it() {
    for mix in [true, false] {
        for capacity in [1, 4, 8] {
            for point in [ProducerPoint::Epoch, ProducerPoint::Rendered, ProducerPoint::PrePush, ProducerPoint::Pushed] {
                for prefix in [2, 137, SAMPLES] {
                    let (control, mut mixer) = mixer(); mixer.config.mix = mix;
                    let continuous = control.sink(); append(&continuous, other, None);
                    let old = control.movie_sink(); append(&old, film, None);
                    let (tx, rx) = queue::channel(capacity);
                    let mut monitor = monitor(mixer);
                    producer_step(&mut monitor, &tx, &control.metrics, capacity, |_| {}).unwrap();
                    let mut callback = callback(rx, control.metrics.clone());
                    expect(&mut callback, 0, prefix, usize::MAX);
                    let metrics = control.metrics.clone();
                    let (entered, seen) = sync_channel(0); let (release, blocked) = sync_channel(0);
                    let worker = thread::spawn(move || {
                        producer_step(&mut monitor, &tx, &metrics, capacity, |p| {
                            if p == point { entered.send(()).unwrap(); blocked.recv_timeout(GUARD).unwrap(); }
                        }).unwrap();
                        (monitor, tx)
                    });
                    seen.recv_timeout(GUARD).unwrap();
                    old.stop();
                    let new = control.movie_sink(); append(&new, new_film, None);
                    old.stop(); // Must not revoke new's immutable identity.
                    let new_id = control.metrics.movie.load(Ordering::Acquire);
                    assert_ne!(new_id, 0);
                    release.send(()).unwrap();
                    let (mut monitor, tx) = worker.join().unwrap();
                    expect(&mut callback, prefix, SAMPLES, (prefix+1)&!1);
                    for i in SAMPLES..2*SAMPLES {
                        // Before render, the new movie belongs to this interval.
                        // After render, use the old interval's no-film premix.
                        let movie = if point == ProducerPoint::Epoch { new_film(i-SAMPLES) } else { 0. };
                        assert_eq!(callback.next(), Some(limited(other(i) + movie)), "{mix}/{capacity}/{point:?}/{prefix}/{i}");
                    }
                    assert!(producer_step(&mut monitor, &tx, &control.metrics, capacity, |_| {}).is_some());
                    for i in 2*SAMPLES..3*SAMPLES {
                        let new_index = if point == ProducerPoint::Epoch { i-SAMPLES } else { i-2*SAMPLES };
                        assert_eq!(callback.next(), Some(limited(other(i)+new_film(new_index))));
                    }
                    assert_eq!(control.metrics.starved.load(Ordering::Relaxed), 0);
                }
            }
        }
    }
}

#[test]
fn stop_and_rearm_during_actual_source_render_keep_the_old_interval_tag() {
    for mix in [true, false] {
        for capacity in [1, 4, 8] {
            let (control, mut mixer) = mixer(); mixer.config.mix = mix;
            let (entered, seen) = sync_channel(0); let (release, blocked) = sync_channel(0);
            let continuous = control.sink();
            append(&continuous, other, Some(Gate { at: 100, entered, release: blocked }));
            let old = control.movie_sink(); append(&old, film, None);
            let (tx, rx) = queue::channel(capacity); let metrics = control.metrics.clone();
            let worker = thread::spawn(move || {
                let mut monitor = monitor(mixer);
                producer_step(&mut monitor, &tx, &metrics, capacity, |_| {}).unwrap();
                (monitor, tx)
            });
            seen.recv_timeout(GUARD).unwrap(); old.stop();
            let new = control.movie_sink(); append(&new, new_film, None);
            release.send(()).unwrap();
            let (mut monitor, tx) = worker.join().unwrap();
            let mut callback = callback(rx, control.metrics.clone());
            expect(&mut callback, 0, SAMPLES, 0);
            producer_step(&mut monitor, &tx, &control.metrics, capacity, |_| {}).unwrap();
            // New sources are admitted at the next buffered interval, so
            // the replacement film starts at its own PCM index zero.
            for i in SAMPLES..2*SAMPLES { assert_eq!(callback.next(), Some(limited(other(i)+new_film(i-SAMPLES)))); }
            assert_eq!(control.metrics.starved.load(Ordering::Relaxed), 0);
        }
    }
}

#[test]
fn dequeue_and_odd_stop_choose_one_premix_for_both_channels() {
    for capacity in [1, 4, 8] {
        let (control, mut mixer) = mixer(); mixer.config.mix = true;
        let continuous = control.sink(); append(&continuous, other, None);
        let movie = control.movie_sink(); append(&movie, film, None);
        let (tx, rx) = queue::channel(capacity); let mut monitor = monitor(mixer);
        producer_step(&mut monitor, &tx, &control.metrics, capacity, |_| {}).unwrap();
        let mut callback = callback(rx, control.metrics.clone());
        assert_eq!(callback.next_with(|| movie.stop()), Some(other(0)));
        assert_eq!(callback.next(), Some(other(1)));
        assert_eq!(control.metrics.starved.load(Ordering::Relaxed), 0);
    }
}

#[test]
fn saturation_uses_independently_limited_remaining_sum_and_capture_is_producer_mix() {
    let (control, mut mixer) = mixer(); mixer.config.mix = true;
    let continuous = control.sink(); append(&continuous, |_| 0.6, None);
    let movie = control.movie_sink(); append(&movie, |_| 0.6, None);
    let mut monitor = monitor(mixer); monitor.record = true;
    let (tx, rx) = queue::channel(4);
    producer_step(&mut monitor, &tx, &control.metrics, 4, |_| {}).unwrap();
    let mut callback = callback(rx, control.metrics.clone());
    assert_eq!(callback.next(), Some(limited(1.2)));
    movie.stop();
    assert_eq!(callback.next(), Some(limited(1.2))); // same stereo pair
    for _ in 2..SAMPLES { assert_eq!(callback.next(), Some(limited(0.6))); }
    assert_ne!(limited(0.6), limited(1.2)-0.6);
    let capture = monitor.data.samples.lock().unwrap();
    assert_eq!(capture.len(), SAMPLES);
    assert!(capture.iter().all(|v| *v == limited(1.2)));
    assert_eq!(control.metrics.starved.load(Ordering::Relaxed), 0);
}

#[test]
fn real_zero_pcm_and_exhaustion_flush_bursts_keep_raw_starvation_contract() {
    for capacity in [1, 4, 8] {
        let (control, mut mixer) = mixer(); mixer.config.mix = true;
        let silent = control.sink(); append(&silent, |_| 0., None);
        let (tx, rx) = queue::channel(capacity); let mut monitor = monitor(mixer);
        for _ in 0..capacity { producer_step(&mut monitor, &tx, &control.metrics, capacity, |_| {}).unwrap(); }
        let mut callback = callback(rx, control.metrics.clone());
        for _ in 0..capacity*SAMPLES { assert_eq!(callback.next(), Some(0.)); }
        assert_eq!(control.metrics.starved.load(Ordering::Relaxed), 0);
        assert_eq!(callback.next(), Some(0.));
        assert_eq!(control.metrics.starved.load(Ordering::Relaxed), 1);
        control.flush(); control.flush();
        assert_eq!(callback.next(), Some(0.)); // finish right of current pair
        assert_eq!(control.metrics.starved.load(Ordering::Relaxed), 1);
        assert_eq!(callback.next(), Some(0.));
        assert_eq!(control.metrics.starved.load(Ordering::Relaxed), 2);
        control.flush(); control.flush(); control.flush();
        assert_eq!(callback.next(), Some(0.));
        assert_eq!(callback.next(), Some(0.));
        assert_eq!(control.metrics.starved.load(Ordering::Relaxed), 3);
        assert_eq!(control.metrics.flush_silence.load(Ordering::Relaxed), 2);
        for _ in 1..SAMPLES { callback.next(); }
        callback.next();
        assert_eq!(control.metrics.starved.load(Ordering::Relaxed), 4);
        assert_eq!(control.metrics.flush_silence.load(Ordering::Relaxed), 2);
    }
}

#[test]
fn global_flush_still_rejects_true_pcm_with_eight_pop_work_bound() {
    let (control, mut mixer) = mixer(); mixer.config.mix = true;
    let continuous = control.sink(); append(&continuous, other, None);
    let (tx, rx) = queue::channel(8); let mut monitor = monitor(mixer);
    for _ in 0..8 { producer_step(&mut monitor, &tx, &control.metrics, 8, |_| {}).unwrap(); }
    control.flush();
    let mut callback = callback(rx, control.metrics.clone()); let mut pops = 0;
    assert_eq!(callback.next_with(|| pops += 1), Some(0.));
    assert_eq!(pops, READY_SLOTS);
    assert_eq!(control.metrics.queued_samples.load(Ordering::Relaxed), 0);
    assert_eq!(control.metrics.starved.load(Ordering::Relaxed), 1);
    assert_eq!(control.metrics.flush_silence.load(Ordering::Relaxed), 1);
    producer_step(&mut monitor, &tx, &control.metrics, 8, |_| {}).unwrap();
    for _ in 1..SAMPLES { assert_eq!(callback.next(), Some(0.)); }
    assert_eq!(callback.next(), Some(other(8*SAMPLES)));
    assert_eq!(control.metrics.starved.load(Ordering::Relaxed), 1);
}

#[test]
fn pcm_storage_layout_is_recorded_without_a_speed_claim() {
    use std::mem::size_of;
    println!("PCM_LAYOUT block={} option_block={} ring_slots={} callback={} mixer={} metrics={} sink={} output={} monitor={}",
        size_of::<Block>(), size_of::<Option<Block>>(), READY_SLOTS,
        size_of::<Callback>(), size_of::<Mixer>(), size_of::<Metrics>(),
        size_of::<Sink>(), size_of::<Output>(), size_of::<Monitor<Mixer>>());
    assert!(!std::mem::needs_drop::<Block>());
}

#[test]
fn normal_sum_order_is_exact_and_movie_free_sum_retains_relative_order() {
    for movie_at in 0..=3 {
        let (control, mut mixer) = mixer(); mixer.config.mix = true;
        let values = [0.7f32, -0.6999999, 0.03125];
        let mut keep = Vec::new(); let mut movie = None;
        let mut ordered = Vec::new();
        for at in 0..=3 {
            if at == movie_at {
                let sink = control.movie_sink(); append(&sink, film, None);
                movie = Some(sink); ordered.push(None);
            }
            if at < 3 {
                let sink = control.sink();
                sink.append(rodio::buffer::SamplesBuffer::new(2, 44100, vec![values[at]; SAMPLES*2]));
                keep.push(sink); ordered.push(Some(values[at]));
            }
        }
        let mut monitor = monitor(mixer);
        let (tx, rx) = queue::channel(4);
        for _ in 0..2 { producer_step(&mut monitor, &tx, &control.metrics, 4, |_| {}).unwrap(); }
        let mut callback = callback(rx, control.metrics.clone());
        for i in 0..SAMPLES {
            let sum = ordered.iter().fold(0f32, |sum, value| sum + value.unwrap_or_else(|| film(i)));
            assert_eq!(callback.next().unwrap().to_bits(), limited(sum).to_bits());
        }
        movie.as_ref().unwrap().stop();
        let sum = values.into_iter().fold(0f32, |sum, value| sum + value);
        for _ in 0..SAMPLES { assert_eq!(callback.next().unwrap().to_bits(), limited(sum).to_bits()); }
        assert_eq!(control.metrics.starved.load(Ordering::Relaxed), 0);
    }
}

#[test]
fn selective_pcm_matches_real_no_movie_reference_through_pause_fades_and_virtual_phase() {
    let make = |with_movie: bool| {
        let (control, mut mixer) = mixer(); mixer.config.mix = true; mixer.config.voices = 1; mixer.config.gain = 0.001;
        let a = control.sink(); a.set_ambient(); a.set_volume(0.7); append(&a, other, None);
        let film_sink = if with_movie { let m = control.movie_sink(); append(&m, film, None); Some(m) } else { None };
        let b = control.sink(); b.set_ambient(); b.set_volume(0.1); append(&b, |i| other(i) * -0.5, None);
        let continuous = control.sink(); append(&continuous, |i| other(i) * 0.25, None);
        (control, monitor(mixer), a, b, continuous, film_sink)
    };
    let (control, mut producer, a, b, _continuous, film) = make(true);
    let (reference_control, mut reference, ra, rb, _rc, _no_movie) = make(false);
    let (tx, rx) = queue::channel(4); let mut callback = callback(rx, control.metrics.clone());
    for interval in 0..9 {
        if interval == 1 { a.set_volume(0.05); b.set_volume(0.8); ra.set_volume(0.05); rb.set_volume(0.8); }
        if interval == 3 { a.pause(); b.pause(); ra.pause(); rb.pause(); }
        if interval == 4 { a.play(); b.play(); ra.play(); rb.play(); }
        if interval == 5 { a.set_volume(1.); b.set_volume(0.1); ra.set_volume(1.); rb.set_volume(0.1); }
        if interval == 7 { a.stop(); ra.stop(); }
        producer_step(&mut producer, &tx, &control.metrics, 4, |_| {}).unwrap();
        let reference_pcm = reference.pcm_block(reference_control.metrics.epoch.load(Ordering::Acquire));
        if interval == 0 { film.as_ref().unwrap().stop(); }
        for expected in reference_pcm.samples { assert_eq!(callback.next().unwrap().to_bits(), expected.to_bits(), "interval {interval}"); }
        assert_eq!(a.get_pos(), ra.get_pos()); assert_eq!(b.get_pos(), rb.get_pos());
    }
    assert_eq!(control.metrics.starved.load(Ordering::Relaxed), 0);
}
