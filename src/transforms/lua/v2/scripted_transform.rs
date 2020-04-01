use crate::{event::Event, transforms::Transform};
use futures01::{
    stream, sync::mpsc::Receiver as FutureReceiver, Async, Future, IntoFuture,
    Stream as FutureStream,
};
use std::{
    mem,
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
    time::Duration,
};
use tokio01::timer::Interval;

#[derive(Clone, Copy, Debug)]
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

#[derive(Debug)]
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
                println!("received msg: {:?}", msg);
                match msg {
                    Message::Process(event) => {
                        runtime.hook_process(event, |event| runtime_output.send(Some(event)).unwrap());
                    }
                    _ => ()
                    // Message::Init => {
                    //     runtime.hook_init(|event| runtime_output.send(Some(event)).unwrap())
                    // }
                    // Message::Process(event) => runtime
                    //     .hook_process(event, |event| runtime_output.send(Some(event)).unwrap()),
                    // Message::Shutdown => {
                    //     runtime.hook_shutdown(|event| runtime_output.send(Some(event)).unwrap());
                    // }
                    // Message::Timer(timer) => runtime
                    //     .timer_handler(timer, |event| runtime_output.send(Some(event)).unwrap()),
                };
                runtime_output.send(None).unwrap();
            }
        });

        let timers = timers_rx.recv().unwrap();
        println!("timers: {:?}", timers);

        ScriptedTransform {
            input,
            output,
            timers,
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

fn make_timer_msgs_stream(timers: &Vec<Timer>) -> MessageStream {
    let mut stream: MessageStream = Box::new(stream::empty());
    for timer in timers {
        stream = Box::new(stream.select(interval_from_timer(*timer)));
    }
    stream
}

impl ScriptedStream {
    fn new(mut transform: ScriptedTransform, input_rx: FutureReceiver<Event>) -> ScriptedStream {
        let timers = mem::take(&mut transform.timers);
        let input_rx: MessageStream = Box::new(
            input_rx
                .map(|event| Message::Process(event))
                .into_future()
                .map(move |(first, rest)| {
                    // The first message is always `Message::Init`.
                    let init_msg = stream::once(Ok(Message::Init));
                    // After it comes the first event, if any.
                    let first_event = first.map_or_else(
                        || -> MessageStream { Box::new(stream::empty()) },
                        |msg| -> MessageStream { Box::new(stream::once(Ok(msg))) },
                    );
                    // Then all other events followed by `Message::Shutdown` message
                    let rest_events_and_shutdown_msg =
                        rest.chain(stream::once(Ok(Message::Shutdown)));
                    // A stream of `Message::Timer(..)` events generated by timers.
                    let timer_msgs = make_timer_msgs_stream(&timers);

                    init_msg
                        .chain(first_event)
                        .chain(rest_events_and_shutdown_msg.select(timer_msgs))
                })
                .map_err(|_| ())
                .into_stream()
                .flatten(),
        );

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
        println!("running `poll`...");
        loop {
            match self.state {
                RuntimeState::Idle => {
                    match self.input_rx.poll() {
                        Ok(Async::Ready(Some(msg))) => {
                            println!("got msg {:?}", msg);
                        }
                        self.transform.input.send(msg).unwrap();
                        self.state = RuntimeState::Processing();
                    }
                }
            }
            // match self.state {
            //     RuntimeState::Idle => match self.input_rx.poll() {
            //         Ok(Async::Ready(Some(msg))) => {
            //             self.transform.input.send(msg).unwrap();
            //             self.state = RuntimeState::Processing;
            //             Ok(Async::Ready(None))
            //         }
            //         other => other.map(|async_| async_.map(|_| None)),
            //     },
            //     RuntimeState::Processing => match self.transform.output.try_recv() {
            //         Ok(Some(event)) => Ok(Async::Ready(Some(event))),
            //         Ok(None) => {
            //             self.state = RuntimeState::Idle;
            //             Ok(Async::Ready(None))
            //         }
            //         Err(_) => Ok(Async::Ready(None)),
            //     },
            // }
        }
    }
}
