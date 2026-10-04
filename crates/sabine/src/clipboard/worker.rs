use std::thread;

use super::{ClipboardContent, Reply, Selection};

/// The system clipboard, reached on a thread of its own: another app may be
/// slow to hand over what it copied, and the window keeps drawing meanwhile.
pub(crate) struct SystemClipboard {
    jobs: crossbeam_channel::Sender<Job>,
}

enum Job {
    Read {
        types: Option<Vec<String>>,
        reply: Reply,
    },
    Write(ClipboardContent),
}

/// One desktop's clipboard, as MIME types and their bytes.
pub(super) trait Pasteboard {
    fn read(types: Option<&[String]>) -> Result<ClipboardContent, String>;
    fn write(content: &ClipboardContent) -> Result<(), String>;
}

impl SystemClipboard {
    pub(super) fn start<P: Pasteboard>() -> Self {
        let (jobs, queue) = crossbeam_channel::unbounded::<Job>();
        thread::spawn(move || {
            for job in queue {
                match job {
                    Job::Read { types, reply } => reply(P::read(types.as_deref())),
                    Job::Write(content) => {
                        if let Err(error) = P::write(&content) {
                            sabine_runtime::report_error("clipboard", error);
                        }
                    }
                }
            }
        });
        Self { jobs }
    }

    pub(crate) fn read(&self, selection: Selection, types: Option<Vec<String>>, reply: Reply) {
        if selection == Selection::Primary {
            reply(Err("The primary selection only exists on Linux".to_string()));
            return;
        }
        let _ = self.jobs.send(Job::Read { types, reply });
    }

    pub(crate) fn write(
        &self,
        selection: Selection,
        content: ClipboardContent,
    ) -> Result<(), String> {
        if selection == Selection::Primary {
            return Err("The primary selection only exists on Linux".to_string());
        }
        let _ = self.jobs.send(Job::Write(content));
        Ok(())
    }
}
