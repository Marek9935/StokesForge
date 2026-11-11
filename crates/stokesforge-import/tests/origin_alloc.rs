// SPDX-License-Identifier: GPL-3.0-or-later

//! The reader never allocates from a size or a count the file declares
//! (invariant 10). One test only: the counting allocator is global to this
//! test binary.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use stokesforge_import::origin::read_project;
use stokesforge_import::origin::synthetic::{
    write_project, Book, Column, Project, Sheet, FIRST_DATASET_OFFSET,
};
use stokesforge_import::origin::OriginDesignation;

struct Counting;

static LARGEST: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards every call to the system allocator unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LARGEST.fetch_max(new_size, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[test]
fn declared_sizes_and_counts_allocate_nothing() {
    let project = Project {
        books: vec![Book {
            short_name: "Book1".into(),
            long_name: String::new(),
            sheets: vec![Sheet {
                name: "Data".into(),
                columns: vec![Column::numbers("A", OriginDesignation::Y, &[1.0, 2.0])],
            }],
            horiba_xml: None,
        }],
        ..Project::default()
    };
    let valid = write_project(&project);

    let mut size = valid.clone();
    size[FIRST_DATASET_OFFSET..FIRST_DATASET_OFFSET + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    // Row count (0x19), value size (0x3D) and data size lie, the data does not.
    let mut rows = valid.clone();
    let header = FIRST_DATASET_OFFSET + 5;
    rows[header + 0x19..header + 0x1D].copy_from_slice(&i32::MAX.to_le_bytes());
    let data_size = header + 0x80 + 1;
    rows[data_size..data_size + 4].copy_from_slice(&u32::MAX.to_le_bytes());

    LARGEST.store(0, Ordering::Relaxed);
    assert!(read_project(&size).is_err());
    assert!(read_project(&rows).is_err());
    assert!(read_project(&valid).is_ok());
    let largest = LARGEST.load(Ordering::Relaxed);
    assert!(
        largest < 1 << 20,
        "largest single allocation: {largest} bytes"
    );
}
