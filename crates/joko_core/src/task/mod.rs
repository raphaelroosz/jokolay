use std::{
    result::Result, sync::{mpsc::{RecvError, SendError}, Arc, Mutex}, thread::JoinHandle
};

//TODO: could this be a wrapper only and a move/copy would not impact content ?
pub struct AsyncTaskGuard<TaskItem, ResultItem>
{
    task_sender: std::sync::mpsc::Sender<TaskItem>,
    result_receiver: std::sync::mpsc::Receiver<ResultItem>,
    thread_task: Option<JoinHandle<()>>,
    thread_nb: Option<JoinHandle<()>>,
    nb: Arc<std::sync::atomic::AtomicI32>,
}

pub type AsyncTask<TaskItem, ResultItem> = Arc<Mutex<AsyncTaskGuard<TaskItem, ResultItem>>>;

impl<TaskItem, ResultItem> AsyncTaskGuard<TaskItem, ResultItem>
where
    TaskItem: Send + 'static,
    ResultItem: Send + 'static,
{
    pub fn new<F>(f: F) -> AsyncTask<TaskItem, ResultItem>
    where
        F: Fn(TaskItem) -> ResultItem + Send + 'static,
    {
        //https://doc.rust-lang.org/rust-by-example/std_misc/channels.html
        let (task_sender, th_task_receiver) = std::sync::mpsc::channel();
        let (th_result_sender, result_receiver) = std::sync::mpsc::channel();
        let (nb_sender, nb_receiver) = std::sync::mpsc::channel();
        let nb = Arc::new(std::sync::atomic::AtomicI32::new(0));
        let res = Arc::new(Mutex::new(Self {
            task_sender,
            result_receiver,
            thread_task: None,
            thread_nb: None,
            nb: Arc::clone(&nb)
        }));
        let thread_task = std::thread::spawn(move || {
            while let Ok(elt) = th_task_receiver.recv() {
                let _guard = scopeguard::guard(0, |_| {
                    nb_sender.send(-1); 
                });
                nb_sender.send(1);
                th_result_sender.send(f(elt));
            }
        });
        let thread_nb = std::thread::spawn(move || {
            while let Ok(elt) = nb_receiver.recv() {
                {
                    nb.fetch_add(elt, std::sync::atomic::Ordering::Relaxed);
                }
            }
        });
        
        {
            let mut t = res.lock().unwrap();
            t.thread_task = Some(thread_task);
            t.thread_nb = Some(thread_nb);
        }
        res
    }
    pub fn send(&self, value: TaskItem) -> Result<(), SendError<TaskItem>> {
        self.task_sender.send(value)
    }
    pub fn recv(&self) -> Result<ResultItem, RecvError> {
        self.result_receiver.recv()
    }

    pub fn count(&self) -> i32 {
        self.nb.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn is_running(&self) -> bool {
        let nb = self.nb.load(std::sync::atomic::Ordering::Relaxed);
        nb != 0
    }
}