use std::io;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;

use tracing_subscriber::layer::SubscriberExt;

#[derive(Clone, Default)]
struct SharedWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
}

struct SharedWriterGuard {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedWriter {
    type Writer = SharedWriterGuard;

    fn make_writer(&'a self) -> Self::Writer {
        SharedWriterGuard {
            buffer: Arc::clone(&self.buffer),
        }
    }
}

impl Write for SharedWriterGuard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer
            .lock()
            .expect("log buffer should lock")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn capture_logs(f: impl FnOnce()) -> String {
    let writer = SharedWriter::default();
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .without_time()
            .with_writer(writer.clone()),
    );
    tracing::subscriber::with_default(subscriber, f);

    let bytes = writer
        .buffer
        .lock()
        .expect("log buffer should lock")
        .clone();
    String::from_utf8(bytes).expect("log output should be utf8")
}
