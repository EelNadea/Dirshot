#![allow(unused)]


#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]


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
    err:String,

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

        let mut app = match self.inner.lock() {
            Ok(app) => app,
            Err(poisoned) => {
                eprintln!("Mutex poisoned: {:?}", poisoned);
                return
            }
        };


        egui::TopBottomPanel::bottom("status_bar").show(context, |ui| {
            ui.add_space(4.0);
            ui.label(&app.selected_directory);
            ui.label(&app.status);
            ui.label(&app.err);
            ui.add_space(4.0);
        });


        egui::CentralPanel::default().show(context, |ui| {
            ui.heading("Directory Snapshotter");


            if ui.button("Choose a directory").clicked() {
                if let Some(folder) = FileDialog::new().pick_folder() {
                   app.root_path = folder.to_string_lossy().to_string();
                   app.selected_directory = format!("[+] Selected directory: {}", app.root_path);
                   app.status = "".to_string();
                   app.err = "".to_string();

                   app.snap1_button_clicked = false;
                   app.snap2_button_clicked = false;
                   app.compare_button_clicked = false;
                }

                else {
                    app.err = "[X] No directory selected!".into();
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
                    app.err = "[X] Please select a directory.".into();
                }
                else {
                    app.err = "".to_string();
                    app.status = "[+] Taking snapshot 1...".into();


                    let thread_app:Arc<Mutex<DirshotApp>> = Arc::clone(&self.inner);
                    thread::spawn(move || {
                        // This delay allows the gui to repaint before the Mutex is locked
                        thread::sleep(std::time::Duration::from_millis(500));
                        take_snap_1(thread_app);
                    });
                }
            }


            if
                ui.button("Snapshot 2").clicked() &&
                app.snap1_button_clicked &&
                !app.snap2_button_clicked
            {
                app.status = "[+] Taking snapshot 2...".into();


                let thread_app = Arc::clone(&self.inner);
                thread::spawn(move || {
                    thread::sleep(std::time::Duration::from_millis(500));
                    take_snap_2(thread_app);
                });
            }


            if 
                ui.button("Compare").clicked() &&
                app.snap2_button_clicked &&
                !app.compare_button_clicked
            {
                app.status = "[+] Comparing...".into();


                let thread_app = Arc::clone(&self.inner);
                thread::spawn(move || {
                    thread::sleep(std::time::Duration::from_millis(500));
                    compare(thread_app);
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
        err:String::new(),

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


fn take_snap_1(thread_app:Arc<Mutex<DirshotApp>>) {

    if let Ok(mut state) = thread_app.lock() {
        let mut output_path:PathBuf = PathBuf::from(&state.root_path);
        output_path.push("Dirshot_Output");

        if let Err(err) = fs::create_dir(&output_path) {
            state.err = format!("[X] Error: Failed to create output directory: {}", err);
        };
        let mut db_path:PathBuf = output_path.clone();
        db_path.push("snapshot.db");


        match Connection::open(db_path) {
            Ok(connection) => {
                make_db_tables(&connection);


                let completion_time:SystemTime = recursive_snap_shot(
                    state.root_path.to_string(),
                    &state.max_depth,
                    1,
                    &connection
                );


                state.snap1_completion_time = completion_time;
                state.status = "[*] Finished snapshot 1".into();
                state.snap1_button_clicked = true;
            },


            Err(err) => {
                if let Ok(mut state) = thread_app.lock() {
                    state.status = format!("[X] Error: Failed to open connection with the database: {}", err);
                }
            }
        };
    }
}


fn take_snap_2(thread_app:Arc<Mutex<DirshotApp>>){

    if let Ok(mut state) = thread_app.lock() {
        let mut output_path:PathBuf = PathBuf::from(&state.root_path);
        output_path.push("Dirshot_Output");

        let mut db_path:PathBuf = output_path.clone();
        db_path.push("snapshot.db");

        // Extract values to move into thread
        let root_path:String = state.root_path.clone();
        let max_depth:u8 = state.max_depth;


        match Connection::open(db_path) {
            Ok(connection) => {
                let completion_time:SystemTime = recursive_snap_shot(
                    state.root_path.to_string(),
                    &state.max_depth,
                    2,
                    &connection
                );
                state.status = "[*] Finished snapshot 2".into();
                state.snap2_button_clicked = true;
            },


            Err(err) => {
                state.status = format!("[X] Error: Failed to open connection with the database: {}", err);
            }
        };
    }
}


fn compare(thread_app:Arc<Mutex<DirshotApp>>) {

    if let Ok(mut state) = thread_app.lock() {
        let mut output_path:PathBuf = PathBuf::from(&state.root_path);
        output_path.push("Dirshot_Output");

        let mut db_path:PathBuf = output_path.clone();
        db_path.push("snapshot.db");


        let mut success:bool = true;
        match Connection::open(db_path) {
            Ok(database) => {
                let snap1_completion_time:SystemTime = state.snap1_completion_time;
                let mut file_groups = std::mem::take(&mut state.file_groups);

                if let Err(err) = hash_based_file_comparison(&database, &mut file_groups, snap1_completion_time) {
                    state.err = format!("[X] Error: Comparison failed: {}", err);
                    success = false;
                    return
                }

                if success {
                    make_analysis_output(&state.root_path, file_groups);
                    let mut report_path:PathBuf = output_path;
                    report_path.push("report.txt");
                    state.status = format!("[*] Finished comparing. You may check {}", report_path.display());
                }

                state.compare_button_clicked = true;
            },


            Err(err) => {
                state.status = format!("[X] Error: Failed to open connection with the database: {}", err);
                success = false;
            }
        };
    }
}
