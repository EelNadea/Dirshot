Dirshot: For Malware Analysts

Inspired by Regshot, Dirshot (Directory Snapshot) is a tool designed to capture the state of a directory 
and its subdirectories at two distinct points in time, typically before and after malware infection. For 
each file, it collects detailed information such as the file path, SHA256 hash, compressed contents, creation 
time, and last modification time. This data is then stored in a database for later analysis.

The comparison process identifies changes between snapshots by grouping files into categories such as 
unchanged, renamed or moved, edited, and new files. It does this primarily by matching SHA256 hashes and 
analyzing file metadata timestamps relative to the first snapshot’s completion time. This helps analysts 
quickly pinpoint modifications, additions, or relocations within the monitored directory, facilitating 
effective malware behavior analysis or system change tracking.