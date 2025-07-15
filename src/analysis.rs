#![allow(unused)]

use std::{
    time::{SystemTime, UNIX_EPOCH},
    fs::File,
    io::Write
};

use rusqlite::Connection;

// Constants for file_groups:Vec<Vec<String>> indices
const UNCHANGED_FILES:usize = 0;
const RENAMED_OR_MOVED_FILES:usize = 1;
const EDITED_FILES:usize = 2;
const NEW_FILES:usize = 3;

pub fn hash_based_file_comparison(
    // Parameters
    database:&Connection, 
    file_groups:&mut [Vec<String>; 4],
    snap1_completion_time:SystemTime
) -> Result<(), rusqlite::Error> {

    let snap1_completion_time:usize = 
        snap1_completion_time
            .duration_since(UNIX_EPOCH)
            .expect("Error: SystemTime convertion to usize failed")
            .as_secs() as usize;

    let mut snap1_query = database.prepare(
        "SELECT file_path FROM snap1_files WHERE sha256 = ?1"
    ).unwrap();

    let mut snap2_query = database.prepare(
        "SELECT file_path, sha256 FROM snap2_files"
    ).unwrap();

    let snap2_rows = snap2_query.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?
        ))
    })?;

    for row in snap2_rows {
        let (snap2_file_path, snap2_sha256) = row?;

        let mut matches = snap1_query.query([&snap2_sha256])?;
        if let Some(matched_row) = matches.next()? {
            // Ideally, there should only be one match, hence the use of a constant index
            let snap1_file_path:String = matched_row.get(0)?;

            if snap2_file_path == snap1_file_path { file_groups[UNCHANGED_FILES].push(snap2_file_path); }
            else { file_groups[RENAMED_OR_MOVED_FILES].push(snap2_file_path); }
        }

        else { 
            let (last_modified, time_created):(usize, usize) = database.query_row(
                "SELECT last_modified, time_created FROM snap2_files WHERE file_path = ?1",
                rusqlite::params![snap2_file_path],
                |row| Ok((row.get(0)?, row.get(1)?))
            ).unwrap();

            if (last_modified > snap1_completion_time) && (time_created != last_modified) { 
                file_groups[EDITED_FILES].push(snap2_file_path); 
            }
            else if time_created > snap1_completion_time { file_groups[NEW_FILES].push(snap2_file_path); }
        }
    }

    Ok(())
}

pub fn make_analysis_output(root_path:&String, file_groups:[Vec<String>; 4]) {
    let mut analysis_output:File = 
        File::create(format!("{}/Dirshot_Output/report.txt", root_path))
        .expect("Error: Output file creation failed");

    analysis_output.write_all(b"Unchanged files:\n");
    for unchanged_file in &file_groups[UNCHANGED_FILES] {
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
