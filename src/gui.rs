#![allow(unused)]


use crate::{

    snapshot::*,
    analysis::*
};


use std::{

    fs,
    time::SystemTime,
    path::PathBuf,
    thread,
    sync::{Arc, Mutex}
};


use eframe::egui::{self, output};
use rusqlite::Connection;
use rfd::FileDialog;


pub struct DirshotApp {

    pub root_path:String,
    pub selected_directory:String,
    pub status:String,

    pub max_depth:u8,

    pub snap1_completion_time:SystemTime,

    pub file_groups:Vec<Vec<String>>,

    pub snap1_button_clicked:bool,
    pub snap2_button_clicked:bool,
    pub compare_button_clicked:bool
}


pub struct SharedDirshotApp {
    
    pub inner:Arc<Mutex<DirshotApp>>
}


impl eframe::App for SharedDirshotApp {

    fn update(&mut self, context:&egui::Context, frame:&mut eframe::Frame) {

        let mut app = self.inner.lock().unwrap();


        egui::TopBottomPanel::bottom("status_bar").show(context, |ui| {

            ui.add_space(4.0);
            ui.label(&app.selected_directory);
            ui.label(&app.status);
            ui.add_space(4.0);
        });


        egui::CentralPanel::default().show(context, |ui| {

            ui.heading("Directory Snapshotter");


            if ui.button("Choose a directory").clicked() {

                if let Some(folder) = FileDialog::new().pick_folder() {

                   app.root_path = folder.to_string_lossy().to_string();
                   app.selected_directory = format!("[+] Selected directory: {}", app.root_path);
                   app.status = "".to_string(); 


                   app.snap1_button_clicked = false;
                   app.snap2_button_clicked = false;
                   app.compare_button_clicked = false;
                }

                else {
                    app.selected_directory = "[X] No directory selected!".into();
                }
            }


            ui.horizontal(|ui| {

                ui.label("Max scan depth:");
                ui.add(
            
                    egui::DragValue::new(&mut app.max_depth).speed(1).range(1..=255)
                );
            });


            ui.separator();     // For layout purposes


            if 
                ui.button("Snapshot 1").clicked() && 
                app.snap1_button_clicked == false
            {

                if app.root_path.is_empty() {

                    app.status = "[X] Please choose a directory to monitor.".into();
                    return;
                }

                else if !app.root_path.is_empty() {

                    app.status = "[+] Taking snapshot 1...".into();


                    let mut output_path:PathBuf = PathBuf::from(&app.root_path);
                    output_path.push("Dirshot_Output");

                    fs::create_dir(&output_path).unwrap();


                    let mut db_path:PathBuf = output_path.clone();
                    db_path.push("snapshot.db");


                    // Extract values to move into thread
                    let root_path:String = app.root_path.clone();
                    let max_depth:u8 = app.max_depth;

                    // Clone shared arc for threading
                    let thread_app = Arc::clone(&self.inner);

                    thread::spawn(move || {

                        let database:Connection = Connection::open(db_path).unwrap();
                        make_db_tables(&database);


                        let completion_time:SystemTime = recursive_snap_shot(

                            root_path, 
                            &max_depth, 
                            1, 
                            &database
                        );


                        let mut state = thread_app.lock().unwrap();
                        state.snap1_completion_time = completion_time;                        
                        state.status = "[*] Finished snapshot 1".into();
                        state.snap1_button_clicked = true;
                    });
                }
            }


            if
                ui.button("Snapshot 2").clicked() &&
                app.snap1_button_clicked == true &&
                app.snap2_button_clicked != true
            {

                app.status = "[+] Taking snapshot 2...".into();


                let mut output_path:PathBuf = PathBuf::from(&app.root_path);
                output_path.push("Dirshot_Output");


                let mut db_path:PathBuf = output_path.clone();
                db_path.push("snapshot.db");


                // Extract values to move into thread
                let root_path:String = app.root_path.clone();
                let max_depth:u8 = app.max_depth;

                // Clone shared arc for threading
                let thread_app = Arc::clone(&self.inner);

                thread::spawn(move || {

                    let database:Connection = Connection::open(db_path).unwrap();
                    make_db_tables(&database);


                    let completion_time:SystemTime = recursive_snap_shot(

                        root_path, 
                        &max_depth, 
                        2, 
                        &database
                    );


                    let mut state = thread_app.lock().unwrap();
                    state.status = "[*] Finished snapshot 2".into();
                    state.snap2_button_clicked = true;
                });
            }


            if 
                ui.button("Compare").clicked() &&
                app.snap2_button_clicked == true &&
                app.compare_button_clicked != true
            {

                app.status = "[+] Comparing...".into();


                let mut output_path:PathBuf = PathBuf::from(&app.root_path);
                output_path.push("Dirshot_Output");


                let mut db_path:PathBuf = output_path.clone();
                db_path.push("snapshot.db");


                // Extract value to move into thread and to satisfy the borrow checker
                let root_path:String = app.root_path.clone();
                let snap1_completion_time:SystemTime = app.snap1_completion_time;
                let mut file_groups:Vec<Vec<String>> = app.file_groups.clone();

                // Clone shared arc for threading
                let thread_app = Arc::clone(&self.inner);

                thread::spawn(move || {

                    let database:Connection = Connection::open(db_path).unwrap();


                    let mut status_msg:String = String::new();
                    let mut success:bool = true;


                    if let Err(err) = hash_based_file_comparison(&database, &mut file_groups, snap1_completion_time) {

                        status_msg = format!("[X] Comparison failed: {}", err);        
                        success = false;    
                    };


                    let mut state = thread_app.lock().unwrap();
                    if success == true {
                        make_analysis_output(&root_path, &file_groups);

                        let mut report_path:PathBuf = PathBuf::from(output_path);
                        report_path.push("report.txt");

                        state.status = format!("[*] Finished comparing. You may check {}", report_path.display()).into();
                    }


                    state.compare_button_clicked = true;
                });
            }
        });
    }
}

