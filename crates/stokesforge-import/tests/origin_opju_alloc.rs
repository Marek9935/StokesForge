// SPDX-License-Identifier: GPL-3.0-or-later

//! The `.opju` reader never allocates from a size or a count the file
//! declares (invariant 10). One test only: the counting allocator is global
//! to this test binary.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use stokesforge_import::origin::read_project;
use stokesforge_import::origin::synthetic::{
    lz4_literal, opju_header_code, put_leb, put_zigzag, write_opju_with_dataset, STRING_TABLE_KEY,
};

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

fn block(flag: u8, rows: u64, items: &[u8]) -> Vec<u8> {
    let mut cells = vec![flag];
    put_leb(&mut cells, rows);
    cells.extend_from_slice(items);
    cells
}

fn run(m: u64, kind: u8) -> Vec<u8> {
    let mut v = vec![0xFF, 0xFF];
    put_leb(&mut v, m);
    v.push(kind);
    v
}

fn file(cells: &[u8], props: &[u8]) -> Vec<u8> {
    write_opju_with_dataset(&opju_header_code("Book1_A"), cells, props)
}

#[test]
fn declared_sizes_and_counts_allocate_nothing() {
    let mut cases: Vec<(&str, Vec<u8>)> = Vec::new();

    let mut repeat = run(1 << 40, 0);
    put_zigzag(&mut repeat, 1 << 40);
    repeat.push(0x64);
    cases.push(("repeat count", file(&block(5, 1 << 40, &repeat), &[])));

    let mut few = run(1, 0);
    put_zigzag(&mut few, 1);
    few.push(0x64);
    cases.push(("row count", file(&block(5, 1 << 50, &few), &[])));

    let mut fpc = run(500_000, 0);
    put_zigzag(&mut fpc, -500_000);
    fpc.extend_from_slice(&[0x0C, 0x11, 0x01, 0x02]);
    cases.push(("FPC count", file(&block(5, 500_000, &fpc), &[])));

    let text = vec![b'a'; 100_000];
    let mut texts = run(2000, 1);
    put_zigzag(&mut texts, 2000);
    put_leb(&mut texts, text.len() as u64);
    texts.extend_from_slice(&text);
    cases.push(("repeated text", file(&block(5, 2000, &texts), &[])));

    let mut payload = u32::MAX.to_le_bytes().to_vec();
    payload.extend_from_slice(&lz4_literal(b"w\x11\x11\x11"));
    let mut props = STRING_TABLE_KEY.to_vec();
    props.extend_from_slice(&[0x05, 0x01]);
    put_leb(&mut props, payload.len() as u64);
    props.push(0);
    props.extend_from_slice(&payload);
    let mut reference = vec![0x01, 0x00];
    put_leb(&mut reference, 1);
    cases.push(("LZ4 size", file(&block(4, 1, &reference), &props)));

    cases.push((
        "zero-run",
        write_opju_with_dataset(&vec![0xBFu8; 20_000], &block(5, 0, &[]), &[]),
    ));

    LARGEST.store(0, Ordering::Relaxed);
    for (name, bytes) in &cases {
        assert!(read_project(bytes).is_err(), "{name}");
    }
    let largest = LARGEST.load(Ordering::Relaxed);
    assert!(
        largest <= 4 << 20,
        "largest single allocation: {largest} bytes"
    );
}
