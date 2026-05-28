use super::start::FavaiAgent;

impl FavaiAgent {
    pub async fn shutdown(self) {
        self._sync_task.abort();
        for task in self._watch_tasks {
            task.abort();
        }
    }
}
