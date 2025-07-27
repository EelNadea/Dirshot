#![allow(unused)]

use std::{
    time::{SystemTime, UNIX_EPOCH},
    fs::File,
    io::Write
};

use crate::{
    FileInfoMap,
    FileInfo,
    insert_files_into_db
};

use rusqlite::{Connection, params};

// Constants for file_groups:Vec<Vec<String>> indices
const REMOVED_FILES:usize = 0;
const RENAMED_OR_MOVED_FILES:usize = 1;
const EDITED_FILES:usize = 2;
const NEW_FILES:usize = 3;


pub fn hash_based_comparison(
    database:&Connection, 
    file_groups:&mut [Vec<String>; 4],
    snap2_file_info_map:&mut FileInfoMap
) -> Result<(), rusqlite::Error> {

    let mut snap1_query = database.prepare(
        "SELECT file_path, sha256 FROM snap1_files"
    ).unwrap();

    let snap1_rows = snap1_query.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,   // file_path
            row.get::<_, String>(1)?    // sha256
        ))
    })?;


    for row in snap1_rows {
        let (snap1_file_path, snap1_sha256) = row?;


        if let Some(snap2_file_info) = snap2_file_info_map.search_map(&snap1_sha256, &snap1_file_path) {
            if
                snap1_sha256 == snap2_file_info.sha256 &&
                snap1_file_path == snap2_file_info.file_path
            { snap2_file_info_map.remove_entry(&snap2_file_info); }

            else if
                snap1_sha256 == snap2_file_info.sha256 &&
                snap1_file_path != snap2_file_info.file_path
            {
                file_groups[RENAMED_OR_MOVED_FILES].push(snap1_file_path);
                snap2_file_info_map.remove_entry(&snap2_file_info);
            }

            else if
                snap1_sha256 != snap2_file_info.sha256 &&
                snap1_file_path == snap2_file_info.file_path
            {
                file_groups[EDITED_FILES].push(snap1_file_path);
                snap2_file_info_map.remove_entry(&snap2_file_info);
            }
        }
    }


    Ok(())
}

pub fn time_based_comparison(
    file_groups:&mut [Vec<String>; 4],
    snap1_completion_time:&SystemTime,
    snap2_file_info_map:&FileInfoMap
) {

    /*
        This hashmap has been chosen arbitrarily. There would be no difference between iterating
        over "by_file_path" or "by_hash" since both hashmaps point to the same data
    */
    for arc in snap2_file_info_map.by_path.values() {
        if arc.time_created >= *snap1_completion_time {
            file_groups[NEW_FILES].push(arc.file_path.clone());
        }
        else if arc.last_modified >= *snap1_completion_time {
            file_groups[EDITED_FILES].push(arc.file_path.clone());
        }
    }
}


pub fn write_file_groups_to_db(
    database:&Connection,
    file_groups:&[Vec<String>; 4],
    snap2_file_info_map:&FileInfoMap
) -> rusqlite::Result<(), rusqlite::Error> {




    Ok(())
}


pub fn make_analysis_output(root_path:&String, file_groups:[Vec<String>; 4]) {
    let mut analysis_output:File = 
        File::create(format!("{}/Dirshot_Output/report.txt", root_path))
        .expect("Error: Output file creation failed");

    analysis_output.write_all(b"Removed files:\n");
    for unchanged_file in &file_groups[REMOVED_FILES] {
        writeln!(analysis_output, "\t{}", unchanged_file).expect("Error: File write failed");
    }
    analysis_output.write_all(b"\n");

    analysis_output.write_all(b"Renamed or moved files:\n");
    for renamed_or_moved_file in &file_groups[RENAMED_OR_MOVED_FILES] {
        writeln!(analysis_output, "\t{}", renamed_or_moved_file).expect("Error: File write failed");
    }
    analysis_output.write_all(b"\n");

    analysis_output.write_all(b"Edited files:\n");
    for renamed_or_moved_file in &file_groups[EDITED_FILES] {
        writeln!(analysis_output, "\t{}", renamed_or_moved_file).expect("Error: File write failed");
    }
    analysis_output.write_all(b"\n");

    analysis_output.write_all(b"New files:\n");
    for new_file in &file_groups[NEW_FILES] {
        writeln!(analysis_output, "\t{}", new_file).expect("Error: File write failed");
    }
}
