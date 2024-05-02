use std::thread::{self, JoinHandle};

use futures::future::OptionFuture;
use slint::{ModelRc, StandardListViewItem, TableColumn, VecModel, Weak};
use sqlx::{query, sqlite::SqliteRow, Row};
use tokio::sync::mpsc::Receiver;

pub enum WorkerMessage {
    ChangeDatabase(String),
    ChangeTable(String),
}

pub fn spawn_background_worker(window_ref: Weak<crate::AppWindow>, mut database_name_receiver: Receiver<WorkerMessage>) -> JoinHandle<()> {
    let handle = thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap().block_on(async {
            let mut database_connection = None;
            let mut schema_getter_future = OptionFuture::default();
            let mut table_getter_future = OptionFuture::default();
            loop {
                tokio::select! {
                    message = database_name_receiver.recv() => {

                        window_ref.upgrade_in_event_loop(|window| {
                            window.set_column_names(ModelRc::default());
                            window.set_table_data(ModelRc::default());
                        }).unwrap();

                        match message {
                            Some(WorkerMessage::ChangeDatabase(new_file_path)) => {
                                let conn = sqlx::SqlitePool::connect(&format!("sqlite:{}", new_file_path)).await.unwrap();
                                database_connection = Some(conn);
                                if let Some(conn) = &database_connection {
                                    let cloned = conn.clone();
                                    schema_getter_future = OptionFuture::from(Some(Box::pin(async move {
                                        return query(r#"SELECT name FROM sqlite_schema WHERE type = "table""#).fetch_all(&cloned).await;
                                    })));
                                }
                            }

                            Some(WorkerMessage::ChangeTable(new_table)) => {
                                if let Some(conn) = &database_connection {
                                    let cloned = conn.clone();
                                    table_getter_future = OptionFuture::from(Some(Box::pin(async move {
                                        let table_info_query = query(&format!("PRAGMA table_info({})", new_table)).fetch_all(&cloned).await;
                                        let table_data_query = query(&format!("SELECT * FROM {} LIMIT 25", new_table)).fetch_all(&cloned).await;
                                        return (table_info_query, table_data_query);
                                    })));
                                }
                            }

                            None => {
                                println!("Channel closed, exiting runtime");
                                return;
                            }
                        }
                    }

                    Some((Ok(cols), rows)) = &mut table_getter_future => {
                        table_getter_future = OptionFuture::default();
                        if cols.is_empty() {
                            window_ref.upgrade_in_event_loop(|window| {
                                window.set_column_names(ModelRc::default());
                                window.set_table_data(ModelRc::default());
                            }).unwrap();
                            break;
                        }
                        let column_names = cols.iter().map(|column| {
                            let mut result = TableColumn::default();
                            result.title = column.get::<&str, _>("name").into();
                            return result;
                        }).collect::<Vec<_>>();

                        window_ref.upgrade_in_event_loop(|window| {
                            window.set_column_names(ModelRc::new(VecModel::from(column_names)));
                        }).unwrap();

                        match rows {
                            Ok(rows) => {
                                let columns = cols.iter().map(|column| {
                                    return (column.get::<&str, _>("name"), column.get::<&str, _>("type"));
                                }).collect::<Vec<_>>();
                                let table_rows = rows.iter().map(|row| {
                                    return columns.iter().map(|(col_name, col_type)| {
                                        return StandardListViewItem::from(row_data_to_text(&row, col_name, col_type).as_str());
                                        //return StandardListViewItem::from(row.get::<&str, _>(col_index));
                                    }).collect::<Vec<_>>();
                                }).collect::<Vec<_>>();
                                window_ref.upgrade_in_event_loop(|window| {
                                    let table_rows = table_rows.into_iter().map(|row_data| {
                                        return ModelRc::new(VecModel::from(row_data));
                                    }).collect::<Vec<_>>();
                                    window.set_table_data(ModelRc::new(VecModel::from(table_rows)));
                                }).unwrap();
                            }
                            Err(_) => {
                                window_ref.upgrade_in_event_loop(|window| {
                                    window.set_table_data(ModelRc::default());
                                }).unwrap();
                            }
                        }
                    }
                    
                    Some(Ok(rows)) = &mut schema_getter_future => {
                        schema_getter_future = OptionFuture::default();
                        let tables = rows.iter().map(|row| {
                            return StandardListViewItem::from(row.get::<&str, _>("name"));
                        }).collect::<Vec<_>>();
                        window_ref.upgrade_in_event_loop(|window| {
                            window.set_database_tables(ModelRc::new(VecModel::from(tables)));
                            window.set_column_names(ModelRc::default());
                        }).unwrap();
                    }

                    else => {
                        println!("Error in query, resetting futures");
                        schema_getter_future = OptionFuture::default();
                        table_getter_future = OptionFuture::default();
                    }
                }
            }
        });
    });
    
    return handle;
}

fn row_data_to_text(row: &SqliteRow, column_name: &str, type_name: &str) -> String {
    match type_name {
        "NULL" => return String::new(),
        "INTEGER" => return row.get::<i64, _>(column_name).to_string(),
        "REAL" => return row.get::<f64, _>(column_name).to_string(),
        "TEXT" => return row.get::<String, _>(column_name),
        _ => return String::from("type not known"),
    }
}