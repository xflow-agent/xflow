use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::events::OutputEvent;
use crate::ui_adapter::UiAdapter;

pub struct ThinkingAnimation {
    running: Arc<AtomicBool>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl ThinkingAnimation {
    pub fn start(ui: Arc<dyn UiAdapter>, show_thinking: bool, dot_max: usize) -> Self {
        let running = Arc::new(AtomicBool::new(true));

        let task = if show_thinking {
            let running_clone = running.clone();
            Some(tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(1000));
                let mut dot_count = 0;
                loop {
                    interval.tick().await;
                    if !running_clone.load(Ordering::Relaxed) {
                        break;
                    }
                    if dot_count < dot_max {
                        ui.output(OutputEvent::ThinkingDot).await;
                        dot_count += 1;
                    }
                }
            }))
        } else {
            None
        };

        Self { running, task }
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }

    pub async fn finish(&mut self) {
        self.stop();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}
