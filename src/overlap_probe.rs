//! Default-off memory contention diagnostic. No simulation or renderer objects cross threads.
use std::{sync::mpsc, thread, time::{Duration, Instant}};
pub(crate) struct Probe {
    worker: Option<(mpsc::SyncSender<bool>, mpsc::Receiver<(f64, u64)>, thread::JoinHandle<()>)>,
    busy: bool,
}
impl Probe {
    pub(crate) fn configured() -> Self {
        #[cfg(feature="vita_renderer")]
        let ms = crate::render::vita_renderer::config().diag_overlap_burn_ms;
        #[cfg(not(feature="vita_renderer"))]
        let ms = 0;
        let worker = if ms == 0 { None } else {
            let (tx, rx) = mpsc::sync_channel(1);
            let (done, result) = mpsc::sync_channel(1);
            let handle = thread::Builder::new().name("overlap-burn".into()).stack_size(512*1024).spawn(move || {
                crate::skin_worker::pin_overlap_burn();
                // 2 MiB exceeds the shared L2; volatile read/write prevents dead-store removal.
                let mut memory = vec![1u32; 512*1024];
                while rx.recv() == Ok(true) {
                    let start=Instant::now(); let mut bytes=0;
                    while start.elapsed() < Duration::from_millis(u64::from(ms)) {
                        for i in (0..memory.len()).step_by(16) {
                            unsafe { let p=memory.as_mut_ptr().add(i); p.write_volatile(p.read_volatile().wrapping_add(1)); }
                            bytes += 8;
                        }
                    }
                    if done.send((start.elapsed().as_secs_f64()*1000., bytes)).is_err() { break; }
                }
            }).expect("overlap burn thread");
            Some((tx,result,handle))
        };
        Self { worker, busy:false }
    }
    pub(crate) fn begin(&mut self) {
        if let Some((tx,_,_))=&self.worker { assert!(!self.busy); tx.send(true).unwrap(); self.busy=true; }
    }
    pub(crate) fn finish(&mut self) {
        if self.busy {
            let (_,rx,_)=self.worker.as_ref().unwrap();
            let (ms,bytes)=rx.recv().expect("overlap burn completion");
            self.busy=false;
            crate::spike_log::overlap_burn(ms,bytes);
        }
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.finish();
        if let Some((tx,_,handle))=self.worker.take() { let _=tx.send(false); handle.join().expect("overlap burn shutdown"); }
    }
}
