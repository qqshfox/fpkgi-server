# Sorting Behavior in FPKGi Server

This document explains the sorting logic implemented in the FPKGi Server's web interface, specifically for the root index (`/`) and directory listings (e.g., `/pkgs/`). The server uses Actix Web to serve static files and custom handlers for directory navigation.

## Current Implementation

Both the root index and directory listings are sorted **case-insensitively** using Rust's `sort_by` method with a case-insensitive comparison. This is implemented in `src/server.rs` as follows:

- **Root Index (`root_index` handler)**:
  - Collects directory names from `ServerConfig.directories.keys()` into a vector.
  - Sorts with: `dir_names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()))`.
  - Displays sorted names as links (e.g., `/Icons`, `/jsons`, `/pkgs`).

- **Directory Listings (`dir_listing` handler)**:
  - Reads directory contents using `fs::read_dir`.
  - Collects filenames into a vector.
  - Sorts with: `file_list.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()))`.
  - Displays sorted files as links (e.g., `afile.pkg`, `FileB.pkg`).

## Why Case-Insensitive Sorting?

The choice of case-insensitive sorting was made for the following reasons:

1. **User-Friendly Experience**:
   - In web interfaces and file browsers (e.g., Windows Explorer, macOS Finder), users expect alphabetical sorting to disregard case. For example, `FileA.pkg` and `fileb.pkg` should appear close together (e.g., `FileA.pkg`, `fileb.pkg`) rather than separated by case (e.g., `FileA.pkg`, ..., `fileb.pkg`).

2. **Readability**:
   - Case-insensitive sorting groups related names (e.g., `Icons` and `icons`, if present) together, enhancing visual coherence and making navigation more intuitive.

3. **Consistency**:
   - Applying the same sorting logic to both the root index (listing directories like `/pkgs`, `/jsons`) and subdirectory listings (e.g., `/pkgs/`) ensures a uniform user experience. Inconsistent sorting (e.g., case-sensitive at root, case-insensitive in directories) could confuse users navigating between levels.

4. **Convention Alignment**:
   - Many web servers (e.g., Apache, Nginx) default to case-insensitive sorting for directory listings, aligning FPKGi Server with common expectations.

## Case-Insensitive vs. Case-Sensitive

- **Case-Insensitive** (Current):
  - Example root: `/Icons`, `/jsons`, `/pkgs` → `/Icons`, `/jsons`, `/pkgs` (order preserved, case ignored).
  - Example dir: `FileA.pkg`, `ZFile.pkg`, `afile.pkg`, `fileb.pkg` → `afile.pkg`, `FileA.pkg`, `fileb.pkg`, `ZFile.pkg`.
  - Benefits: Intuitive for humans, groups similar names.

- **Case-Sensitive** (Alternative):
  - Example root: `/Icons`, `/jsons`, `/pkgs` → `/Icons`, `/jsons`, `/pkgs` (uppercase `I` before lowercase `j`, `p`).
  - Example dir: `FileA.pkg`, `ZFile.pkg`, `afile.pkg`, `fileb.pkg` → `FileA.pkg`, `ZFile.pkg`, `afile.pkg`, `fileb.pkg`.
  - Use Case: Matches strict ASCII/Unicode order, useful for programmatic exactness or case-sensitive filesystems.

Case-insensitive was chosen as it prioritizes human usability over strict technical ordering, fitting the web-based file server context.

## Why Consistent Sorting?

Uniform sorting avoids disjointed navigation. For example:
- If root sorted case-sensitively (`/Icons`, `/pkgs`, `/jsons`) but directories sorted case-insensitively (`afile.pkg`, `FileB.pkg`), the transition between views would feel inconsistent.
- Consistency ensures predictability, especially since both the root index and directory listings serve as navigational aids.

## Customization Options

For future modifications, sorting can be adjusted:

1. **Switch to Case-Sensitive**:
   - Replace `sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()))` with `sort()` or `sort_by(|a, b| a.cmp(b))`.
   - Result: Strict alphabetical order respecting case (uppercase before lowercase).

2. **Mixed Sorting**:
   - Use case-sensitive for root (e.g., to reflect command-line order) and case-insensitive for directories (e.g., for user browsing).
   - Example: Root as `/Icons`, `/jsons`, `/pkgs`; Dir as `afile.pkg`, `FileB.pkg`.

3. **Alternative Criteria**:
   - Sort by modification time, size, or other metadata by modifying the `sort_by` closure (e.g., `sort_by(|a, b| a.metadata().unwrap().modified().cmp(&b.metadata().unwrap().modified()))`).

## Current Behavior Example

- **Command**: `cargo run -- serve --dirs "jsons:/data/jsons" --dirs "Icons:/data/icons" --dirs "pkgs:/data/packages"`
- **Root Index (`/`)**:
  ```
  Available Directories
  - /Icons
  - /jsons
  - /pkgs
  ```
- **Directory Listing (`/pkgs/`)**:
  ```
  Directory Contents
  - afile.pkg
  - FileB.pkg
  - ZFile.pkg
  ```
