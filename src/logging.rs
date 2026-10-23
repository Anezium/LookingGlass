//! Whole-record logging before stdout/stderr reach the Vita debug and file sinks.
//!
//! Rust serializes each stream separately. That does not serialize println!
//! against eprintln! when both descriptors feed PrincessLog/the same log file.
//! Formatting directly into Write also emits literals and values separately.
use std::{fmt, io::{self, Write}, sync::Mutex};

#[derive(Clone, Copy)]
pub(crate) enum Stream { Out, Err }

struct Logger<S>(Mutex<S>);

impl<S: Sink> Logger<S> {
    fn line(&self, stream: Stream, args: fmt::Arguments<'_>) -> io::Result<()> {
        // Format before locking: Display may itself log. Never expose partial
        // format fragments or a separate newline to either sink.
        let mut record = fmt::format(args);
        record.push('\n');
        let mut sink = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        sink.line(stream, record.as_bytes())
    }
}

trait Sink {
    fn line(&mut self, stream: Stream, record: &[u8]) -> io::Result<()>;
}

struct Stdio;
impl Sink for Stdio {
    fn line(&mut self, stream: Stream, record: &[u8]) -> io::Result<()> {
        // Take BOTH stream locks in a fixed order. Dependency diagnostics still
        // use std's macros; these locks keep them outside an engine record too.
        let stdout = io::stdout();
        let stderr = io::stderr();
        let mut out = stdout.lock();
        let mut err = stderr.lock();
        match stream {
            Stream::Out => out.write_all(record),
            Stream::Err => err.write_all(record),
        }
    }
}

static LOGGER: Logger<Stdio> = Logger(Mutex::new(Stdio));

#[cfg_attr(not(target_os = "vita"), allow(dead_code))]
pub(crate) fn line(stream: Stream, args: fmt::Arguments<'_>) {
    // Preserve std printing's failure semantics, after releasing the sink lock.
    if let Err(error) = LOGGER.line(stream, args) {
        panic!("failed writing engine log: {error}");
    }
}

// Host command output keeps its test harness capture behavior. The host tests
// below exercise the same Logger and Stdio implementations used on Vita.
#[cfg(target_os = "vita")]
macro_rules! println {
    () => { $crate::logging::line($crate::logging::Stream::Out, format_args!("")) };
    ($($arg:tt)*) => { $crate::logging::line($crate::logging::Stream::Out, format_args!($($arg)*)) };
}
#[cfg(target_os = "vita")]
macro_rules! eprintln {
    () => { $crate::logging::line($crate::logging::Stream::Err, format_args!("")) };
    ($($arg:tt)*) => { $crate::logging::line($crate::logging::Stream::Err, format_args!($($arg)*)) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashSet, sync::{Arc, Barrier}};

    #[derive(Default)]
    struct FragmentedSinks { udp: Vec<u8>, file: Vec<u8>, calls: usize }
    impl Sink for FragmentedSinks {
        fn line(&mut self, _: Stream, record: &[u8]) -> io::Result<()> {
            assert_eq!(record.last(), Some(&b'\n'));
            self.calls += 1;
            // Deliberately yield between EVERY byte in both downstream sinks.
            // Even short writes/fanout must stay inside the record's lock.
            for &byte in record {
                self.udp.push(byte);
                std::thread::yield_now();
                self.file.push(byte);
            }
            Ok(())
        }
    }

    #[test]
    fn logging_concurrent_stdout_stderr_are_whole_records_in_both_sinks() {
        let logger = Arc::new(Logger(Mutex::new(FragmentedSinks::default())));
        let barrier = Arc::new(Barrier::new(8));
        let threads = (0..8).map(|writer| {
            let logger = logger.clone(); let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                for sequence in 0..500 {
                    let stream = if writer % 2 == 0 { Stream::Out } else { Stream::Err };
                    logger.line(stream, format_args!("writer={writer} sequence={sequence} payload={} END", "x".repeat(128))).unwrap();
                }
            })
        }).collect::<Vec<_>>();
        for thread in threads { thread.join().unwrap(); }
        let sinks = logger.0.lock().unwrap();
        assert_eq!(sinks.calls, 4000);
        assert_eq!(sinks.udp, sinks.file);
        let actual = String::from_utf8(sinks.file.clone()).unwrap();
        let lines = actual.lines().collect::<HashSet<_>>();
        assert_eq!(actual.lines().count(), 4000);
        assert_eq!(lines.len(), 4000);
        for writer in 0..8 { for sequence in 0..500 {
            assert!(lines.contains(format!("writer={writer} sequence={sequence} payload={} END", "x".repeat(128)).as_str()));
        }}
    }

    #[test]
    fn logging_formats_before_lock_and_recovers_after_format_panic() {
        struct Broken;
        impl fmt::Display for Broken {
            fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result { panic!("format failed") }
        }
        let logger = Logger(Mutex::new(FragmentedSinks::default()));
        assert!(std::panic::catch_unwind(|| logger.line(Stream::Out, format_args!("{Broken}"))).is_err());
        logger.line(Stream::Err, format_args!("next record")).unwrap();
        assert_eq!(logger.0.lock().unwrap().file, b"next record\n");
    }
}
