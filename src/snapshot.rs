use std::{
    io::{self, Write, Cursor},
    fs::{self, Metadata},
    path::Path,
    time::SystemTime,
    collections::HashMap,
    sync::Arc,
    mem
};


use rusqlite::Connection;
use sha256::digest;
use zstd::encode_all;


// Buffer for file_info_batch
// file_info_batch can be found in functions recursive_scan_snap1 and recursive_scan_snap2
const BATCH_BUFFER:u8 = 100;

pub struct FileInfo {
    pub file_path:String,
    pub depth:u8,
    pub compressed_bytes:Vec<u8>,
    pub sha256:String,

    pub last_modified:SystemTime,
    pub time_created:SystemTime
}


impl FileInfo {
    pub fn new() -> Self {
        Self {
            // Default values
            file_path: String::new(),
            depth: 0,
            compressed_bytes: Vec::new(),
            sha256: String::new(),

            last_modified: SystemTime::now(),
            time_created: SystemTime::now()
        }
    }


    pub fn build_file_info_struct(
        file_path:String,
        depth:u8,
        file_meta:Metadata
    ) -> FileInfo {

        let raw_bytes:Vec<u8> = fs::read(&file_path).unwrap_or_else(|err| {
            eprintln!("Error: Unable to read the raw bytes: {:?}", err);
            Vec::new()
        });

        let compressed_bytes:Vec<u8> = zstd::encode_all(Cursor::new(&raw_bytes), 3).unwrap_or_else(|err| {
            eprintln!("Error: Unable to compress the bytes: {:?}", err);
            Vec::new()
        });


        let sha256:String = digest(&raw_bytes);


        let last_modified:SystemTime = file_meta.modified().unwrap_or_else(|err| {
            eprintln!("Error: Unable to retrieve modification time: {:?}", err);
            SystemTime::now() // Fallback value
        });

        let time_created:SystemTime = file_meta.created().unwrap_or_else(|err| {
            eprintln!("Error: Unable to retrieve birth time: {:?}", err);
            SystemTime::now()
        });


        FileInfo {
            file_path,
            depth,
            compressed_bytes,
            sha256,

            last_modified,
            time_created
        }
    }
}


pub fn scan_dir_snap1(
    path:String,
    file_info_batch:&mut [FileInfo; BATCH_BUFFER as usize],
    batch_count:&mut u8,
    scanned_files_count:&mut u32,
    dir_container:&mut Vec<String>,
    depth:&u8,
    database:&mut Connection
) {

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

            dir_container.push(dir_path);
        }

        else if
            entry_metadata.is_file() && 
            entry_metadata.len() <= 1024 * 1024 * 1024   // 1GB
        { 
            let file_path:String = entry.path().display().to_string();
            
            let file_info:FileInfo = FileInfo::build_file_info_struct(
                file_path,
                *depth,
                entry_metadata
            );

            if *batch_count < BATCH_BUFFER {
                file_info_batch[*batch_count as usize] = file_info;
                *batch_count += 1;
            }
            else {
                // Resetting file_info_batch is not needed since batch_counter will overwrite the previous data
                insert_files_into_db(database, &*file_info_batch, 1);
                *scanned_files_count += BATCH_BUFFER as u32;
                *batch_count = 0;

                file_info_batch[*batch_count as usize] = file_info;
                *batch_count += 1;
            }
        }
    }
}


pub fn recursive_scan_snap1(
    root_path:String,
    max_depth:&u8,
    database:&mut Connection
) ->  (SystemTime, u32) {

    /*
        Traverses the file system using a breadth-first search strategy.

        Instead of the traditional recursive depth-first traversal, this function performs
        a level-order scan of the directory tree starting from 'root_path'. Each depth
        level is stored in 'dir_container', and traversal continues level-by-level
        until 'max_depth' is reached or no more subdirectories are found.

        This implementation takes advantage of Rust’s Vec allocation strategy, where pushing
        elements into a vector typically causes its capacity to double (or grow geometrically)
        when more space is needed. By reusing the same vector and leaving previously processed
        entries as empty strings via std::mem::take, this approach minimizes unnecessary
        allocations and benefits from the amortized growth of the vector. As a result, new
        entries are added efficiently to the tail, leveraging preallocated capacity and reducing
        the frequency of costly reallocations.
    */    
    
    let mut current_depth:u8 = 0;
    let mut dir_container:Vec<String> = Vec::new();

    let mut file_info_batch:[FileInfo; BATCH_BUFFER as usize] = std::array::from_fn(|_| FileInfo::new());
    let mut batch_count:u8 = 0;  // Keeps count of the non-trivial FileInfo members
    let mut scanned_files_count:u32 = 0;

    scan_dir_snap1(
        root_path,
        &mut file_info_batch,
        &mut batch_count,
        &mut scanned_files_count,
        &mut dir_container,
        &current_depth,
        database
    );


    let mut start:usize = 0;
    while
        (current_depth + 1) != *max_depth &&    // max_depth has a minimum value of 1
        start != dir_container.len()
    {
        let end:usize = dir_container.len();   // The len function has O(1) time complexity
        dir_container.reserve_exact(end*2);


        current_depth += 1;
        for i in start..end {
            let sub_dir:String = mem::take(&mut dir_container[i]);    // Take ownership and leave an empty string

            scan_dir_snap1(
                sub_dir,
                &mut file_info_batch,
                &mut batch_count,
                &mut scanned_files_count,
                &mut dir_container,
                &current_depth,
                database
            );
        }


        start = end;
    }

    // Send the rest, as file_info_batch is not guaranteed to be full everytime
    if batch_count < BATCH_BUFFER {
        insert_files_into_db(database, &file_info_batch[..batch_count as usize], 1);
        scanned_files_count += batch_count as u32;
    }


    // Return completion time and the number of scanned files
    (SystemTime::now(), scanned_files_count)
}


pub struct FileInfoMap {
    // This allows lookup to the same data (owned references) via 2 different keys
    pub by_path:HashMap<String, Arc<FileInfo>>,
    pub by_hash:HashMap<String, Arc<FileInfo>>
}


impl FileInfoMap {
    pub fn new_with_capacity(capacity:usize) -> Self {
        Self {
            by_path: HashMap::with_capacity(capacity),
            by_hash: HashMap::with_capacity(capacity)
        }
    }


    pub fn insert_file(&mut self, file_info:FileInfo) {
        let arc_file_info = Arc::new(file_info);
        self.by_path.insert(arc_file_info.file_path.clone(), Arc::clone(&arc_file_info));
        self.by_hash.insert(arc_file_info.sha256.clone(), arc_file_info);
    }


    pub fn search_by_path(&self, file_path:&String) -> Option<Arc<FileInfo>> {
        self.by_path.get(file_path).cloned()
    }
    pub fn search_by_hash(&self, sha256:&String) -> Option<Arc<FileInfo>> {
        self.by_hash.get(sha256).cloned()
    }
    pub fn search_map(&self, path:&String, hash:&String) -> Option<Arc<FileInfo>> {
        self.search_by_hash(hash).or_else(|| self.search_by_path(path))
    }


    pub fn remove_entry(&mut self, file_info:&FileInfo) {
        self.by_path.remove(&file_info.file_path);
        self.by_hash.remove(&file_info.sha256);
    }
}


pub fn scan_dir_snap2(
    file_info_map:&mut FileInfoMap,
    path:String,
    dir_container:&mut Vec<String>,
    scanned_files_count:&mut u32,
    depth:&u8,
    database:&Connection
) {

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

            dir_container.push(dir_path);
        }

        else if
            entry_metadata.is_file() &&
            entry_metadata.len() <= 1024 * 1024 * 1024   // 1GB
        {
            let file_path:String = entry.path().display().to_string();
            if file_path.contains("Dirshot_Output") { continue; }

            let mut file_info:FileInfo = FileInfo::build_file_info_struct(
                file_path,
                *depth,
                entry_metadata
            );

            file_info_map.insert_file(file_info);
            *scanned_files_count += 1;
        }
    }
}


pub fn recursive_scan_snap2(
    root_path:String,
    max_depth:&u8,
    database:&Connection
) -> (FileInfoMap, u32) {

    let mut depth:u8 = 0;
    let mut dir_container:Vec<String> = Vec::new();


    let estimated_files:usize = ((*max_depth as usize) * (*max_depth as usize)) * 500;
    let mut file_info_map:FileInfoMap = FileInfoMap::new_with_capacity(estimated_files);
    let mut scanned_files_count:u32 = 0;

    scan_dir_snap2(
        &mut file_info_map,
        root_path,
        &mut dir_container,
        &mut scanned_files_count,
        &depth,
        database
    );


    let mut start:usize = 0;
    while
        (depth + 1) != *max_depth &&
        start != dir_container.len()
    {
        let end:usize = dir_container.len();
        dir_container.reserve_exact(end*2);


        depth += 1;
        for i in start..end {
            let sub_dir:String = mem::take(&mut dir_container[i]);

            scan_dir_snap2(
                &mut file_info_map,
                sub_dir,
                &mut dir_container,
                &mut scanned_files_count,
                &depth,
                database
            );
        }


        start = end;
    }


    (file_info_map, scanned_files_count)
}


pub fn insert_files_into_db(
    database:&mut Connection,
    file_infos:&[FileInfo],
    snap_instance:u8
) -> Result<(), rusqlite::Error> {
    let table_name = 
        if snap_instance == 1 { "snap1_files" } 
        else { "snap2_files" };


    let query = format!(
        "INSERT INTO {} (file_path, depth, compressed_bytes, sha256, last_modified, time_created)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
         table_name
    );

    let transaction = database.transaction()?;
    let mut statement = transaction.prepare(&query)?;


    for file_info in file_infos {
        statement.execute(rusqlite::params![
            file_info.file_path,
            file_info.depth,

            file_info.compressed_bytes,
            file_info.sha256,

            file_info.last_modified.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
            file_info.time_created.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
        ])?;
    }

    mem::drop(statement);
    transaction.commit()?;


    Ok(())
}


pub fn make_db_tables(database:&Connection) {
    database.execute_batch(r#"
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
