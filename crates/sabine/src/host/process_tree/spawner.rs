//! Starts the children that end with the app from one thread that lives as
//! long as the app, since Linux ends a child that asked to end with its
//! parent as soon as the thread that started it ends.

use std::{
    io,
    process::{Child, Command},
    sync::OnceLock,
};

type Request = (Command, crossbeam_channel::Sender<io::Result<Child>>);

pub(super) fn spawn(command: Command) -> io::Result<Child> {
    static SPAWNER: OnceLock<crossbeam_channel::Sender<Request>> = OnceLock::new();
    let spawner = SPAWNER.get_or_init(|| {
        let (requests, received) = crossbeam_channel::unbounded::<Request>();
        std::thread::Builder::new()
            .name("sabine-spawner".into())
            .spawn(move || {
                for (mut command, answer) in received {
                    let _ = answer.send(command.spawn());
                }
            })
            .expect("the spawner thread starts");
        requests
    });
    let (answer, answered) = crossbeam_channel::bounded(1);
    spawner
        .send((command, answer))
        .map_err(|_| io::Error::other("the spawner thread ended"))?;
    answered
        .recv()
        .map_err(|_| io::Error::other("the spawner thread ended"))?
}
