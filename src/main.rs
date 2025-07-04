#![allow(unused)]

mod gui;
mod snapshot;
mod analysis;


use std::time::SystemTime;


fn main() -> Result<(), eframe::Error> {

    let mut file_groups:Vec<Vec<String>> = vec![

        Vec::new(), // 0: Unchanged files. Same path, same hash
        Vec::new(), // 1: Renamed or moved files. Same hash, different path
        Vec::new(), // 2: Edited files. Same path, different hash
        Vec::new()  // 3: New files
    ];

    let max_depth:u8 = 1;   // Default value


    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "Dirshot",
        options,
        Box::new(|creation_context| {
            
            Ok(Box::new(gui::DirshotApp {

                root_path:String::new(),
                selected_directory:String::new(),
                status:String::new(),

                max_depth,

                snap1_completion_time: SystemTime::now(),   // Placeholder

                file_groups,

                snap1_button_clicked:false,
                snap2_button_clicked:false,
                compare_button_clicked:false
            }))
        })
    )
}
