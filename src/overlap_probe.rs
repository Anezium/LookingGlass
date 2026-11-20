//! Default-off memory contention diagnostic. No simulation or renderer objects cross threads.
use std::{sync::mpsc, thread, time::{Duration, Instant}};
enum Event { Started, Done(Instant, Instant, u64) }
pub(crate) struct Probe {
    worker: Option<(mpsc::SyncSender<bool>, mpsc::Receiver<Event>, thread::JoinHandle<()>)>,
    busy: bool,
}
impl Probe {
    pub(crate) fn configured() -> Self {
        #[cfg(feature="vita_renderer")]
        let ms = {
            let config = crate::render::vita_renderer::config();
            if config.diag_overlap { config.diag_overlap_burn_ms } else { 0 }
        };
        #[cfg(not(feature="vita_renderer"))]
        let ms = 0;
        let worker = if ms == 0 { None } else {
            let (tx, rx) = mpsc::sync_channel(1);
            let (done, result) = mpsc::sync_channel(1);
            let handle = thread::Builder::new().name("overlap-burn".into()).stack_size(512*1024).spawn(move || {
                crate::skin_worker::pin_reserved_cpu1();
                crate::skin_worker::inventory("overlap-burn", Some(0x20000));
                // 2 MiB exceeds the shared L2; volatile read/write prevents dead-store removal.
                let mut memory = vec![1u32; 512*1024];
                while rx.recv() == Ok(true) {
                    let start=Instant::now(); let mut bytes=0;
                    if done.send(Event::Started).is_err() { break; }
                    while start.elapsed() < Duration::from_millis(u64::from(ms)) {
                        for i in (0..memory.len()).step_by(16) {
                            unsafe { let p=memory.as_mut_ptr().add(i); p.write_volatile(p.read_volatile().wrapping_add(1)); }
                            bytes += 8;
                        }
                    }
                    if done.send(Event::Done(start, Instant::now(), bytes)).is_err() { break; }
                }
            }).expect("overlap burn thread");
            Some((tx,result,handle))
        };
        Self { worker, busy:false }
    }
    pub(crate) fn begin(&mut self) {
        if let Some((tx,_,_))=&self.worker { assert!(!self.busy); tx.send(true).unwrap(); self.busy=true;
            assert!(matches!(self.worker.as_ref().unwrap().1.recv().unwrap(), Event::Started)); }
    }
    pub(crate) fn finish(&mut self) {
        if self.busy {
            let render_end=Instant::now();
            let (_,rx,_)=self.worker.as_ref().unwrap();
            let Event::Done(start,end,bytes)=rx.recv().expect("overlap burn completion") else { panic!("unexpected burn start"); };
            self.busy=false;
            crate::spike_log::overlap_burn(end.duration_since(start).as_secs_f64()*1000.,
                render_end.min(end).saturating_duration_since(start).as_secs_f64()*1000.,bytes);
        }
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.finish();
        if let Some((tx,_,handle))=self.worker.take() { let _=tx.send(false); handle.join().expect("overlap burn shutdown"); }
    }
}

#[cfg(all(test,feature="vita_renderer"))]
mod tests {
    use super::*;
    #[test]
    fn off_allocates_no_worker_and_busy_drop_joins() {
        crate::render::vita_renderer::with_config(|c| c.diag_overlap_burn_ms=0, || {
            let p=Probe::configured(); assert!(p.worker.is_none());
        });
        crate::render::vita_renderer::with_config(|c| {
            c.sim_overlap = true;
            c.diag_overlap_burn_ms = 1;
        }, || assert!(Probe::configured().worker.is_none()));
        crate::render::vita_renderer::with_config(|c| {
            c.diag_overlap = true;
            c.diag_overlap_burn_ms = 1;
        }, || {
            let mut p=Probe::configured();
            for _ in 0..3 { p.begin(); p.finish(); }
            p.begin(); // Drop must drain completion before requesting shutdown.
            drop(p);
        });
    }
}
