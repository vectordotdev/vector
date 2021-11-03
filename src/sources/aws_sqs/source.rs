use crate::shutdown::ShutdownSignal;
use crate::Pipeline;
use aws_sdk_sqs::Client as SqsClient;

pub struct SqsSource {
    pub client: SqsClient,
    pub queue_url: String,
}

impl SqsSource {
    pub async fn run(self, out: Pipeline, shutdown: ShutdownSignal) -> Result<(), ()> {
        let x = self
            .client
            .receive_message()
            .queue_url(&self.queue_url)
            .max_number_of_messages(10)
            //TODO: this will lower CPU / HTTP requests in low load scenarios
            // .wait_time_seconds(x);
            .send()
            .await
            .unwrap();
        todo!()
        // let mut handles = Vec::new();
        // for _ in 0..self.state.client_concurrency {
        //     let process =
        //         IngestorProcess::new(Arc::clone(&self.state), out.clone(), shutdown.clone());
        //     let fut = async move { process.run().await };
        //     let handle = tokio::spawn(fut.in_current_span());
        //     handles.push(handle);
        // }
        //
        // // Wait for all of the processes to finish.  If any one of them panics, we resume
        // // that panic here to properly shutdown Vector.
        // for handle in handles.drain(..) {
        //     if let Err(e) = handle.await {
        //         if e.is_panic() {
        //             panic::resume_unwind(e.into_panic());
        //         }
        //     }
        // }
        //
        // Ok(())
    }
}
