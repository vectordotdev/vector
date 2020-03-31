use crate::{event::Event, transforms::Transform};
use futures01::{stream, sync::mpsc::Receiver as FutureReceiver, Async, Stream as FutureStream};
use std::{
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
    time::Duration,
};
use tokio01::timer::Interval;

#[derive(Clone, Copy)]
pub struct Timer {
    id: u32,
    interval_seconds: u64,
}

pub trait ScriptedRuntime {
    fn hook_init<F>(&mut self, _emit_fn: F)
    where
        F: FnMut(Event) -> (),
    {
    }

    fn hook_process<F>(&mut self, event: Event, emit_fn: F)
    where
        F: FnMut(Event) -> ();

    fn hook_shutdown<F>(&mut self, _emit_fn: F)
    where
        F: FnMut(Event) -> (),
    {
    }

    fn timer_handler<F>(&mut self, _timer: Timer, _emit_fn: F)
    where
        F: FnMut(Event) -> (),
    {
    }

    fn timers(&self) -> Vec<Timer> {
        Vec::new()
    }
}

pub struct ScriptedTransform {
    input: SyncSender<Message>,
    output: Receiver<Option<Event>>,
    timers: Vec<Timer>,
}

enum Message {
    Init,
    Process(Event),
    Shutdown,
    Timer(Timer),
}

impl ScriptedTransform {
    pub fn new<F, T>(create_runtime: F) -> ScriptedTransform
    where
        F: FnOnce() -> T + Send + 'static,
        T: ScriptedRuntime,
    {
        let (input, runtime_input) = mpsc::sync_channel(0);
        let (runtime_output, output) = mpsc::sync_channel(0);

        // One-off channel to read statically defined list of timers from the runtime.
        let (timers_tx, timers_rx) = mpsc::sync_channel(0);

        thread::spawn(move || {
            let mut runtime = create_runtime();
            timers_tx.send(runtime.timers()).unwrap();

            for msg in runtime_input {
                match msg {
                    Message::Init => {
                        runtime.hook_init(|event| runtime_output.send(Some(event)).unwrap())
                    }
                    Message::Process(event) => runtime
                        .hook_process(event, |event| runtime_output.send(Some(event)).unwrap()),
                    Message::Shutdown => {
                        runtime.hook_shutdown(|event| runtime_output.send(Some(event)).unwrap())
                    }
                    Message::Timer(timer) => runtime
                        .timer_handler(timer, |event| runtime_output.send(Some(event)).unwrap()),
                };
                runtime_output.send(None).unwrap();
            }
        });

        ScriptedTransform {
            input,
            output,
            timers: timers_rx.recv().unwrap(),
        }
    }
}

impl Transform for ScriptedTransform {
    // used only in tests
    fn transform(&mut self, event: Event) -> Option<Event> {
        let mut out = Vec::new();
        self.transform_into(&mut out, event);
        assert!(out.len() <= 1);
        out.into_iter().next()
    }

    // used only in tests
    fn transform_into(&mut self, output: &mut Vec<Event>, event: Event) {
        self.input.send(Message::Process(event)).unwrap();
        while let Some(event) = self.output.recv().unwrap() {
            output.push(event);
        }
    }

    fn transform_stream(
        self: Box<Self>,
        input_rx: FutureReceiver<Event>,
    ) -> Box<dyn FutureStream<Item = Event, Error = ()> + Send>
    where
        Self: 'static,
    {
        Box::new(ScriptedStream::new(*self, input_rx))
    }
}

enum RuntimeState {
    Processing,
    Idle,
}

type MessageStream = Box<dyn FutureStream<Item = Message, Error = ()> + Send>;

struct ScriptedStream {
    transform: ScriptedTransform,
    input_rx: MessageStream,
    state: RuntimeState,
}

fn interval_from_timer(timer: Timer) -> impl FutureStream<Item = Message, Error = ()> + Send {
    Interval::new_interval(Duration::new(timer.interval_seconds, 0))
        .map(move |_| Message::Timer(timer))
        .map_err(|_| ())
}

impl ScriptedStream {
    fn new(transform: ScriptedTransform, input_rx: FutureReceiver<Event>) -> ScriptedStream {
        let mut input_rx: MessageStream = Box::new(
            input_rx
                .map(|event| Message::Process(event))
                .chain(stream::once(Ok(Message::Shutdown))),
        );
        for timer in transform.timers.iter() {
            input_rx = Box::new(input_rx.select(interval_from_timer(*timer)));
        }
        // `Message::Init` should come before any other message, including those from timers.
        input_rx = Box::new(stream::once(Ok(Message::Init)).chain(input_rx));

        ScriptedStream {
            transform,
            input_rx,
            state: RuntimeState::Idle,
        }
    }
}

impl FutureStream for ScriptedStream {
    type Item = Event;
    type Error = ();

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        match self.state {
            RuntimeState::Idle => match self.input_rx.poll() {
                Ok(Async::Ready(Some(msg))) => {
                    self.transform.input.send(msg).unwrap();
                    self.state = RuntimeState::Processing;
                    Ok(Async::Ready(None))
                }
                other => other.map(|async_| async_.map(|_| None)),
            },
            RuntimeState::Processing => match self.transform.output.try_recv() {
                Ok(Some(event)) => Ok(Async::Ready(Some(event))),
                Ok(None) => {
                    self.state = RuntimeState::Idle;
                    Ok(Async::Ready(None))
                }
                Err(_) => Ok(Async::Ready(None)),
            },
        }
    }
}
