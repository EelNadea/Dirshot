#![allow(unused)]


mod snapshot;
mod analysis;


use snapshot::*;
use analysis::*;


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


struct DirshotApp {

    root_path:String,
    selected_directory:String,
    status:String,

    max_depth:u8,

    snap1_completion_time:SystemTime,

    file_groups:[Vec<String>; 4],

    snap1_button_clicked:bool,
    snap2_button_clicked:bool,
    compare_button_clicked:bool
}


struct SharedDirshotApp {
    
    inner:Arc<Mutex<DirshotApp>>
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
                !app.snap1_button_clicked
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
                app.snap1_button_clicked &&
                !app.snap2_button_clicked
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
                app.snap2_button_clicked &&
                !app.compare_button_clicked
            {

                app.status = "[+] Comparing...".into();


                let mut output_path:PathBuf = PathBuf::from(&app.root_path);
                output_path.push("Dirshot_Output");


                let mut db_path:PathBuf = output_path.clone();
                db_path.push("snapshot.db");


                // Extract value to move into thread and to satisfy the borrow checker
                let root_path:String = app.root_path.clone();
                let snap1_completion_time:SystemTime = app.snap1_completion_time;
                let mut file_groups:[Vec<String>; 4] = app.file_groups.clone();

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
                    if success {
                        make_analysis_output(&root_path, file_groups);

                        let mut report_path:PathBuf = PathBuf::from(output_path);
                        report_path.push("report.txt");

                        state.status = format!("[*] Finished comparing. You may check {}", report_path.display());
                    }


                    state.compare_button_clicked = true;
                });
            }
        });
    }
}


fn main() -> Result<(), eframe::Error> {

    let mut file_groups:[Vec<String>; 4] = [

        Vec::new(), // 0: Unchanged files. Same path, same hash
        Vec::new(), // 1: Renamed or moved files. Same hash, different path
        Vec::new(), // 2: Edited files. Same path, different hash
        Vec::new()  // 3: New files
    ];

    let max_depth:u8 = 1;   // Default value


    let dirshot_app:DirshotApp = DirshotApp {

        root_path:String::new(),
        selected_directory:String::new(),
        status:String::new(),

        max_depth,

        snap1_completion_time:SystemTime::now(),    // Placeholder

        file_groups,
    
        snap1_button_clicked:false,
        snap2_button_clicked:false,
        compare_button_clicked:false
    };


    let shared_app = Arc::new(Mutex::new(dirshot_app));
    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "Dirshot",
        options,
        Box::new(move |_creation_context| {
            
            Ok(Box::new(SharedDirshotApp {
            
                inner:Arc::clone(&shared_app)
            }))
        })
    )
}
