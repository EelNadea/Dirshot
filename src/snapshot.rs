use std::{

    io::{self, Write, Cursor},
    fs::{self, Metadata},
    path::Path,
    time::SystemTime
};


use sha256::digest;
use rusqlite::Connection;
use zstd;


// **********************************************************************************************************
pub struct FileInfo {

    file_path:String,
    depth:u8,               
    compressed_bytes:Vec<u8>,
    sha256:String,

    last_modified:SystemTime,
    time_created:SystemTime
}


pub struct DirInfo {

    dir_path:String,   
    depth:u8,              // first_sub_dirs = 0, second_sub_dirs = 1, etc...

    last_modified:SystemTime,
    time_created:SystemTime
}


pub fn build_file_info_struct(
    
    // Parameters
    file_path:String, 
    depth:u8,
    file_meta:Metadata

) -> FileInfo {
    
    let raw_bytes:Vec<u8> = fs::read(&file_path).unwrap();


    let compressed_bytes:Vec<u8> = zstd::encode_all(Cursor::new(&raw_bytes), 3).unwrap();
    let sha256:String = digest(&raw_bytes);


    let last_modified:SystemTime = file_meta.modified().unwrap();
    let time_created:SystemTime = file_meta.created().unwrap();


    FileInfo {

        file_path,
        depth,
        compressed_bytes,
        sha256,

        last_modified,
        time_created
    }
}


pub fn build_dir_info_struct(

    // Parameters
    dir_path:String,
    depth:&u8,
    dir_meta:Metadata

) -> DirInfo {

    let last_modified:SystemTime= dir_meta.modified().unwrap();
    let time_created:SystemTime = dir_meta.created().unwrap();


    DirInfo {

        dir_path,
        depth: *depth,

        last_modified,
        time_created
    }
}
// **********************************************************************************************************


// **********************************************************************************************************
pub fn indiv_snap_shot(
    
    // Parameters
    path:String,
    dir_container:&mut Vec<Vec<String>>,
    depth:&u8,
    snap_instance:&u8,
    database:&Connection

) {

    let mut sub_dirs_container:Vec<String> = Vec::new();
    

    let entries = match fs::read_dir(path) {
        
        Ok(entry) => entry,
        Err(_) => return
    };


    for entry in entries {

        let entry = entry.unwrap();
        let entry_metadata:Metadata = match fs::metadata(entry.path()) {

            Ok(entry_metadata) => entry_metadata,
            Err(_) => continue,
        };


        if entry_metadata.is_dir() {
            
            let dir_path:String = entry.path().display().to_string();
            if dir_path.contains("Dirshot_Output") { continue; }        // Skip this directory

            sub_dirs_container.push(dir_path.clone());


            let dir_info:DirInfo = build_dir_info_struct(dir_path, &depth, entry_metadata);
            insert_dir_into_db(&database, dir_info, *snap_instance);
        }

        else if 
            entry_metadata.is_file() && 
            entry_metadata.len() <= 200 * 1024 * 1024   // 200MB
        { 
            let file_path:String = entry.path().display().to_string();

            
            let file_info:FileInfo = 
                    build_file_info_struct(
                        file_path, 
                        *depth, 
                        entry_metadata
                    );


            insert_file_into_db(&database, file_info, *snap_instance);
        }
    }
    

    dir_container[*depth as usize].extend(sub_dirs_container);
}


pub fn recursive_snap_shot(
    
    root_path:String,
    max_depth:&u8,
    snap_instance:u8, 
    database:&Connection

) ->  SystemTime {

    /*
        Traverses the file system using a breadth-first search strategy.

        Instead of the traditional recursive depth-first traversal, this function performs
        a level-order scan of the directory tree starting from 'root_path'. Each depth
        level is stored in 'dir_container', and traversal continues level-by-level
        until 'max_depth' is reached or no more subdirectories are found.
    */    

    
    let mut depth:u8 = 0;
    let mut dir_container:Vec<Vec<String>> = Vec::new();
    dir_container.push(Vec::new());                         // Index = depth

    indiv_snap_shot(root_path, &mut dir_container, &depth, &snap_instance, &database);


    while depth != *max_depth {

        // Index = depth + 1
        dir_container.push(Vec::new());

        
        /* 
            Clone the current depth to avoid complicating the structure of the program. 
            This is a reasonable trade off since it does not accumulate, but rather, 
            it gets dropped once the loop starts over
        */
        let sub_dirs:Vec<String> = dir_container[depth as usize].clone();
        if sub_dirs.is_empty() { return SystemTime::now(); }    // Returns completion time


        depth += 1;
        for sub_dir in sub_dirs {

            indiv_snap_shot(sub_dir, &mut dir_container, &depth, &snap_instance, &database);
        }
    }


    return SystemTime::now();
}
// **********************************************************************************************************


// **********************************************************************************************************
pub fn insert_file_into_db(database:&Connection, file_info:FileInfo, snap_instance:u8) {

    let table_name = 
        if snap_instance == 1 { "snap1_files" } 
        else { "snap2_files" };


    database.execute(

        &format!(
            "INSERT INTO {} (file_path, depth, compressed_bytes, sha256, last_modified, time_created)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
             table_name
        ),


        rusqlite::params![
            file_info.file_path,
            file_info.depth,

            file_info.compressed_bytes,
            file_info.sha256,

            file_info.last_modified.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs().to_string(),
            file_info.time_created.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs().to_string()
        ]
    ).unwrap();
}


pub fn insert_dir_into_db(database:&Connection, dir_info:DirInfo, snap_instance:u8) {

    let table_name = 
        if snap_instance == 1 { "snap1_dirs" } 
        else { "snap2_dirs" };


    database.execute(

        &format!(

            "INSERT INTO {} (dir_path, depth, last_modified, time_created)
             VALUES (?1, ?2, ?3, ?4)", 
             table_name
        ),


        rusqlite::params![
            dir_info.dir_path,
            dir_info.depth,

            dir_info.last_modified.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs().to_string(),
            dir_info.time_created.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs().to_string()
        ]
    ).unwrap();
}


pub fn make_db_tables(database:&Connection) {

    database.execute_batch(r#"

        CREATE TABLE IF NOT EXISTS snap1_dirs (

            dir_path TEXT NOT NULL,
            depth INTEGER NOT NULL,
            last_modified TEXT NOT NULL,
            time_created TEXT NOT NULL
        );


        CREATE TABLE IF NOT EXISTS snap2_dirs (

            dir_path TEXT NOT NULL,
            depth INTEGER NOT NULL,
            last_modified TEXT NOT NULL,
            time_created TEXT NOT NULL
        );


        CREATE TABLE IF NOT EXISTS snap1_files (
        
            file_path TEXT NOT NULL,
            depth INTEGER NOT NULL,
            compressed_bytes BLOB NOT NULL,
            sha256 TEXT NOT NULL,
            last_modified INTEGER NOT NULL,
            time_created INTEGER NOT NULL
        );


        CREATE TABLE IF NOT EXISTS snap2_files (
        
            file_path TEXT NOT NULL,
            depth INTEGER NOT NULL,
            compressed_bytes BLOB NOT NULL,
            sha256 TEXT NOT NULL,
            last_modified INTEGER NOT NULL,
            time_created INTEGER NOT NULL
        );
    "#).unwrap();
}
// **********************************************************************************************************
