mod worker;

use std::env::current_dir;

use slint::Model;
use tokio::sync::mpsc::channel;

slint::include_modules!();

fn main() {
    let ui = AppWindow::new().unwrap();

    let (sender, receiver) = channel(5);

    let handle = ui.as_weak();
    let sender_clone = sender.clone();
    ui.on_request_path_change(move || {
        let window = handle.unwrap();

        window.set_has_open_picker(true);
        let sender_clone = sender_clone.clone();

        slint::spawn_local(async move {
            let picker = rfd::AsyncFileDialog::new()
                .add_filter("SQLite databases", &["db"])
                .set_directory(current_dir().unwrap())
                .pick_file()
                .await;
            if let Some(path) = picker {
                let path = String::from(path.path().to_str().unwrap());
                window.set_database_path(path.clone().into());
                sender_clone.send(worker::WorkerMessage::ChangeDatabase(path)).await.unwrap();
            }
            window.set_has_open_picker(false);
        }).unwrap();
    });

    let handle = ui.as_weak();
    let sender_clone = sender.clone();
    ui.on_request_change_table(move |table_index| {
        let window = handle.unwrap();
        let table_name = window.get_database_tables().row_data(table_index as usize).unwrap().text;
        let sender_clone = sender_clone.clone();
        slint::spawn_local(async move {
            return sender_clone.send(worker::WorkerMessage::ChangeTable(table_name.into())).await;
        }).unwrap();
    });

    let handle = ui.as_weak();
    let background_worker = worker::spawn_background_worker(handle, receiver);

    ui.run().unwrap();
    
    // Drop and close all references to the sender to kill the background thread
    drop(ui);
    drop(sender);
    
    // Allow for cleanup
    background_worker.join().unwrap();
}

// TODO next time
// Query databases for columns
