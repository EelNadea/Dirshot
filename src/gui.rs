#![allow(unused)]


use crate::snapshot::*;
use crate::analysis::*;


use std::fs;
use std::time::SystemTime;
use std::path::PathBuf;


use eframe::egui;
use eframe::egui::output;
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


impl eframe::App for DirshotApp {

    fn update(&mut self, context:&egui::Context, frame:&mut eframe::Frame) {

        egui::TopBottomPanel::bottom("status_bar").show(context, |ui| {

            ui.add_space(4.0);
            ui.label(&self.selected_directory);
            ui.label(&self.status);
            ui.add_space(4.0);
        });


        egui::CentralPanel::default().show(context, |ui| {

            ui.heading("Directory Snapshotter");


            if ui.button("Choose a directory").clicked() {

                if let Some(folder) = FileDialog::new().pick_folder() {

                   self.root_path = folder.to_string_lossy().to_string();
                   self.selected_directory = format!("[+] Selected directory: {}", self.root_path);
                   self.status = "".to_string(); 


                   self.snap1_button_clicked = false;
                   self.snap2_button_clicked = false;
                   self.compare_button_clicked = false;
                }

                else {
                    self.selected_directory = "[X] No directory selected!".into();
                }
            }


            ui.horizontal(|ui| {

                ui.label("Max scan depth:");
                ui.add(
            
                    egui::DragValue::new(&mut self.max_depth).speed(1).range(1..=255)
                );
            });


            ui.separator();     // For layout purposes


            if 
                ui.button("Snapshot 1").clicked() && 
                self.snap1_button_clicked == false
            {

                if self.root_path.is_empty() {

                    self.status = "[X] Please choose a directory to monitor.".into();
                    return;
                }

                else if !self.root_path.is_empty() {

                    let mut output_path:PathBuf = PathBuf::from(&self.root_path);
                    output_path.push("Dirshot_Output");

                    fs::create_dir(&output_path).unwrap();


                    let mut db_path:PathBuf = output_path.clone();
                    db_path.push("snapshot.db");


                    let database:Connection = Connection::open(db_path).unwrap();
                    make_db_tables(&database);


                    self.snap1_completion_time = recursive_snap_shot(self.root_path.clone(), &self.max_depth,  1, &database);


                    self.status = "[*] Finished snapshot 1".into();
                    self.snap1_button_clicked = true;
                }
            }


            if
                ui.button("Snapshot 2").clicked() &&
                self.snap1_button_clicked == true &&
                self.snap2_button_clicked != true
            {
                let mut output_path:PathBuf = PathBuf::from(&self.root_path);
                output_path.push("Dirshot_Output");


                let mut db_path:PathBuf = output_path.clone();
                db_path.push("snapshot.db");


                let database:Connection = Connection::open(db_path).unwrap();


                let snap2_completion_time:SystemTime = recursive_snap_shot(self.root_path.clone(), &self.max_depth, 2, &database);


                self.status = "[*] Finished snapshot 2".into();
                self.snap2_button_clicked = true;
            }


            if 
                ui.button("Compare").clicked() &&
                self.snap2_button_clicked == true &&
                self.compare_button_clicked != true
            {
                let mut output_path:PathBuf = PathBuf::from(&self.root_path);
                output_path.push("Dirshot_Output");


                let mut db_path:PathBuf = output_path.clone();
                db_path.push("snapshot.db");


                let database:Connection = Connection::open(db_path).unwrap();


                hash_based_file_comparison(&database, &mut self.file_groups, self.snap1_completion_time);
                make_analysis_output(&self.root_path, &self.file_groups);


                let mut report_path:PathBuf = PathBuf::from(output_path);
                report_path.push("report.txt");


                self.status = format!("[*] Finished comparing. You may check {}", report_path.display()).into();
                
                
                self.compare_button_clicked = true;
            }
        });
    }
}
