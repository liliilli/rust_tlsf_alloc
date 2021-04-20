#![feature(test)]
extern crate dy_tlsf;
extern crate test;

use std::alloc::{self, GlobalAlloc};

use dy_tlsf::*;
use test::Bencher;

#[global_allocator]
static GLOBAL: TLSFAllocator = TLSFAllocator::new();

#[bench]
fn tlsf_allocator_allocation(b: &mut Bencher) {
    use std::vec::Vec;

    //let allocator: TLSFAllocator = TLSFAllocator::new();
    b.iter(|| {
        let mut pointers: Vec<*mut u8> = vec![];

        // Allocation test
        for i in 1..1024 {
            unsafe {
                let layout = alloc::Layout::from_size_align(i * 32, 16).unwrap();
                let ptr = GLOBAL.alloc(layout);
                assert!(
                    ptr.is_null() == false,
                    "Ptr must not be null and validly allocated."
                );
                pointers.push(ptr);
            }
        }

        for (i, ptr) in pointers.into_iter().enumerate() {
            unsafe {
                let layout = alloc::Layout::from_size_align(i * 32, 16).unwrap();
                GLOBAL.dealloc(ptr, layout);
            }
        }
    });
}
