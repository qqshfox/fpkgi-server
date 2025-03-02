# Resolving 404 and Directory Listing Issues in FPKGi Server

This document chronicles the iterative process to resolve `404 Not Found` errors and directory listing issues in the FPKGi Server's web interface, encountered while transitioning from `actix-files` default behavior to custom sorted directory listings. The goal was to maintain file downloads with `actix-files` features (range requests, content types) while ensuring sorted listings for top-level directories and subfolders.

## Initial State

Originally, the server used `actix-files` with `.show_files_listing()` for static file serving and directory listings:

- **File Downloads**: Worked (e.g., `/icons/file.png` → `200 OK` with range support).
- **Directory Listings**: Worked but unsorted (e.g., `/icons/` → `200 OK` with default listing).
- **Code**: `src/server.rs` with `Files::new("/icons", path).show_files_listing()`.

**Issue**: Listings weren’t sorted alphabetically, prompting customization.

## Step 1: Custom Sorted Listings

To sort directory listings:

- **Change**: Removed `.show_files_listing()` and added a custom `dir_listing` handler with `web::resource("/{path:.*}/").route(web::get().to(dir_listing))`.
- **Result**:
  - Listings sorted (e.g., `/icons/` → `200 OK` with sorted files).
  - Subfolders failed (e.g., `/icons/subfolder` → `"unable to render directory without index file"`).
- **Cause**: `Files` intercepted subfolder requests without trailing slashes before `dir_listing` could handle them, expecting an index file.

## Step 2: Redirect Non-Trailing-Slash Paths

To fix subfolder access without trailing slashes:

- **Change**: Added `dir_redirect` with `web::resource("/{path:.*}").route(web::get().to(dir_redirect))` to redirect `/icons/subfolder` to `/icons/subfolder/`.
- **Result**:
  - Subfolder listings worked with redirect (e.g., `/icons/subfolder` → `301` → `/icons/subfolder/` → `200 OK`).
  - File downloads failed (e.g., `/icons/file.png` → `404 Not Found`).
- **Cause**: Broad `/{path:.*}` route caught file requests, returning `404` before `Files` could serve them.

## Step 3: Custom File Serving Attempt

To restore file downloads:

- **Change**: Replaced `Files` with a single `static_handler` (`/{path:.*}`) to handle both directories and files manually.
- **Result**:
  - Listings and downloads worked (e.g., `/icons/subfolder/` → `200 OK`, `/icons/file.png` → `200 OK`).
  - Lost `actix-files` features (range requests, content types).
- **Cause**: Manual file serving bypassed `actix-files`’ optimized handling.

## Step 4: Revert to Files with Route Order Fix

To regain `actix-files` features:

- **Change**: Restored `Files` before custom routes, expecting `dir_listing` and `dir_redirect` to override directories.
- **Result**:
  - File downloads worked (e.g., `/icons/file.png` → `200 OK` with range support).
  - Subfolders failed again (e.g., `/icons/subfolder` → `"unable to render directory without index file"`).
- **Cause**: `Files` intercepted directory requests first, not allowing custom routes to handle them.

## Step 5: Specific Routes Attempt

To prioritize directory handling:

- **Change**: Used specific routes (e.g., `/icons/`, `/icons/subfolder/`) before `Files`, dynamically registering subfolders.
- **Result**:
  - Top-level listings worked (e.g., `/icons/` → `200 OK`).
  - File downloads worked (e.g., `/icons/file.png` → `200 OK`).
  - Subfolder listings failed (e.g., `/icons/new%20dir` → `404` or error due to path mismatch).
- **Cause**: URL-encoded paths (e.g., `new%20dir`) weren’t decoded properly, mismatching filesystem paths.

## Step 6: Final Fix with URL Decoding

To fix subfolder listings:

- **Change**: Added `percent_decode_str` in `dir_listing` and `dir_redirect` to decode paths (e.g., `new%20dir` → `new dir`), adjusted route handling.
- **Result**:
  - Top-level listings worked (e.g., `/icons/` → `200 OK`).
  - Subfolder listings worked (e.g., `/icons/new%20dir/` → `200 OK`, `/icons/new%20dir` → `301` → `200 OK`).
  - File downloads worked (e.g., `/icons/new%20dir/file.png` → `200 OK` with range support).
- **Solution**: Specific directory routes catch decoded paths, `Files` serves files, breaking the cycle.

## Final Configuration

```rust
// src/server.rs (simplified)
HttpServer::new(move || {
    let mut app = App::new()
        .route("/", web::get().to(root_index));
    // Specific directory routes
    for name in directories.keys() {
        app = app.service(web::resource(&format!("/{}/", name)).route(web::get().to(dir_listing)));
        app = app.service(web::resource(&format!("/{}", name)).route(web::get().to(dir_redirect)));
        // Subfolder routes
        if let Ok(entries) = fs::read_dir(&directories[name]) {
            for entry in entries.filter_map(Result::ok) {
                if entry.path().is_dir() {
                    let subpath = entry.file_name().to_string_lossy().to_string();
                    app = app.service(web::resource(format!("/{}/{}/", name, subpath)).route(web::get().to(dir_listing)));
                    app = app.service(web::resource(format!("/{}/{}", name, subpath)).route(web::get().to(dir_redirect)));
                }
            }
        }
    }
    // Files after specific routes
    for (name, path) in &config.directories {
        app = app.service(Files::new(&format!("/{}", name), path).prefer_utf8(true).use_last_modified(true).use_etag(true));
    }
    app
})
```

## Key Lessons

- **Route Specificity**: Broad patterns (e.g., `/{path:.*}`) caused overlaps; exact routes (e.g., `/icons/new dir/`) resolved conflicts.
- **Order Matters**: `Files` must follow custom directory routes to serve files without interfering with listings.
- **URL Decoding**: Decoding `%20` to spaces was critical for matching filesystem paths.
