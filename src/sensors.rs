// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

//! Sensor value sources.
//!
//! Only implementation is a file-based value provider with simple key-value pairs.

use log::{debug, error, info, warn};
use notify::event::{CreateKind, ModifyKind, RemoveKind};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::ops::DerefMut;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::{Arc, RwLock, mpsc};

/// Read all sensor value source files from the given path and stort monitoring for changes.
///
/// The source path is either a single sensor source file or a directory containing multiple sensor
/// source files.
///
/// The source path is monitored for changes in a separate thread.
/// All updated files are automatically read and stored in the shared HashMap.
///
/// # Arguments
///
/// * `source_path`: Single source file path or a directory path.
/// * `values`: a shared, reader-writer lock protected HashMap
///
/// returns: Result<(), Error>
pub fn start_file_slurper<P: Into<PathBuf>>(
    source_path: P,
    values: Arc<RwLock<HashMap<String, String>>>,
) -> anyhow::Result<()> {
    let dir_path = source_path.into();
    // read existing file(s)
    {
        let mut val = values.write().expect("Failed to lock values");
        read_path(&dir_path, val.deref_mut())?;
    }

    let file_values = values.clone();

    std::thread::spawn(move || {
        // watch sensor file/directory for changes
        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to initialize watcher: {e}");
                exit(1);
            }
        };

        info!("Starting sensor file watcher for {dir_path:?}");
        if let Err(e) = watcher.watch(&dir_path, RecursiveMode::NonRecursive) {
            error!("Failed to start file watcher: {e}");
            exit(1);
        }

        // Block forever, printing out events as they come in
        for res in rx {
            let event = match res {
                Ok(event) => event,
                Err(e) => {
                    warn!("watch error: {e:?}");
                    continue;
                }
            };
            match event.kind {
                EventKind::Create(CreateKind::File) => {
                    debug!("New sensor file: {:?}", event.paths);
                }
                EventKind::Modify(ModifyKind::Data(_)) => {
                    for path in event.paths.iter() {
                        if path.extension().unwrap_or_default() != "txt" {
                            continue;
                        }
                        debug!("Modified sensor file: {path:?}");
                        let mut val = file_values.write().expect("Poisoned sensor RwLock");

                        if let Err(e) = read_from_file(path, val.deref_mut()) {
                            warn!("Failed to read sensor file {path:?}: {e}");
                            continue;
                        }
                    }
                }
                EventKind::Remove(RemoveKind::File) => {
                    debug!("Removed sensor file: {:?}", event.paths);
                }
                _ => {}
            }
        }
    });

    Ok(())
}

/// Read a single key-value-based source file or all source file for a given directory path.
///
/// # Arguments
///
/// * `path`: Single source file path or a directory path.
/// * `values`: HashMap to store all read key-value pairs.
///
/// returns: Result<(), Error>
fn read_path<P: AsRef<Path>>(path: P, values: &mut HashMap<String, String>) -> anyhow::Result<()> {
    let path = path.as_ref();

    if !path.try_exists()? {
        return Ok(());
    }

    if path.is_file() {
        return read_from_file(path, values);
    }

    for entry in fs::read_dir(path)? {
        let path = entry?.path();

        if path.is_file() && path.extension().unwrap_or_default() == "txt" {
            if let Err(e) = read_from_file(&path, values) {
                warn!("Failed to read sensor file {path:?}: {e}");
            }
        }
    }

    Ok(())
}

/// Read a key-value-based sensor source file and store content in the provided hashmap.
///
/// - Empty lines are skipped
/// - Lines starting with # are skipped
/// - Key-value pairs must be separated by `:`
/// - All keys and values are trimmed
///
/// # Arguments
///
/// * `path`: file path to read
/// * `values`: HashMap to store read key-value pairs.
///
/// returns: Result<(), Error>
fn read_from_file<P: AsRef<Path>>(
    path: P,
    values: &mut HashMap<String, String>,
) -> anyhow::Result<()> {
    info!("Reading sensor file {:?}", path.as_ref());

    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            values.insert(key.trim().to_string(), value.trim().to_string());
        } else {
            warn!("Skipping invalid entry in sensor value file: {line}");
        }
    }

    Ok(())
}
