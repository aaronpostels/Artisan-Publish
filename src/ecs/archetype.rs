use std::alloc::{alloc, dealloc, realloc, Layout};
use std::cell::UnsafeCell;
use std::ptr::NonNull;
use std::sync::atomic::AtomicU32;

use super::entity::Entity;

pub struct BlobVec {
    data: NonNull<u8>,
    len: usize,
    cap: usize,
    item_layout: Layout,
    drop_item: Option<unsafe fn(*mut u8)>,
}

unsafe impl Send for BlobVec {}
unsafe impl Sync for BlobVec {}

impl BlobVec {
    pub fn new(
        item_layout: Layout,
        drop_item: Option<unsafe fn(*mut u8)>,
        initial_cap: usize,
    ) -> Self {
        if item_layout.size() == 0 {
            return Self {
                data: unsafe {
                    NonNull::new_unchecked(item_layout.align() as *mut u8)
                },
                len: 0,
                cap: usize::MAX,
                item_layout,
                drop_item,
            };
        }

        let (data, cap) = if initial_cap == 0 {
            (NonNull::dangling(), 0)
        } else {
            let layout = Self::array_layout(item_layout, initial_cap);
            let ptr = unsafe { alloc(layout) };
            (NonNull::new(ptr).expect("BlobVec: allocation failed"), initial_cap)
        };

        Self { data, len: 0, cap, item_layout, drop_item }
    }

    #[inline(always)] pub fn len(&self) -> usize { self.len }
    #[inline(always)] pub fn is_empty(&self) -> bool { self.len == 0 }
    #[inline(always)] pub fn capacity(&self) -> usize { self.cap }
    #[inline(always)] pub fn item_layout(&self) -> Layout { self.item_layout }
    #[inline(always)] pub fn item_size(&self) -> usize { self.item_layout.size() }

    #[inline(always)]
    pub fn as_ptr<T>(&self) -> *const T {
        self.data.as_ptr().cast::<T>()
    }

    #[inline(always)]
    pub unsafe fn as_mut_ptr<T>(&self) -> *mut T {
        self.data.as_ptr().cast::<T>()
    }

    #[inline(always)]
    pub unsafe fn get_raw(&self, row: usize) -> *mut u8 {
        debug_assert!(row < self.len);
        unsafe { self.data.as_ptr().add(row * self.item_layout.size()) }
    }

    #[inline]
    pub unsafe fn push_raw(&mut self, value_ptr: *const u8) {
        if self.item_layout.size() == 0 {
            self.len += 1;
            return;
        }
        if self.len == self.cap {
            self.grow();
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                value_ptr,
                self.data.as_ptr().add(self.len * self.item_layout.size()),
                self.item_layout.size(),
            );
        }
        self.len += 1;
    }

    #[inline]
    pub fn push_typed<T>(&mut self, value: T) {
        unsafe { self.push_raw((&raw const value).cast::<u8>()) };
        std::mem::forget(value);
    }

    #[inline]
    pub unsafe fn swap_remove_drop(&mut self, row: usize) {
        debug_assert!(row < self.len);
        if self.item_layout.size() == 0 {
            self.len -= 1;
            return;
        }
        let size = self.item_layout.size();
        let slot = unsafe { self.data.as_ptr().add(row * size) };

        if let Some(drop_fn) = self.drop_item {
            unsafe { drop_fn(slot) };
        }

        self.len -= 1;
        if row < self.len {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.data.as_ptr().add(self.len * size),
                    slot,
                    size,
                );
            }
        }
    }

    #[inline]
    pub unsafe fn swap_remove_forget(&mut self, row: usize, out_ptr: *mut u8) {
        debug_assert!(row < self.len);
        let size = self.item_layout.size();
        if size == 0 {
            self.len -= 1;
            return;
        }
        let slot = unsafe { self.data.as_ptr().add(row * size) };
        unsafe { std::ptr::copy_nonoverlapping(slot, out_ptr, size) };
        self.len -= 1;
        if row < self.len {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.data.as_ptr().add(self.len * size),
                    slot,
                    size,
                );
            }
        }
    }

    #[inline]
    pub unsafe fn swap_remove_forget_no_copy(&mut self, row: usize) {
        debug_assert!(row < self.len);
        let size = self.item_layout.size();
        if size == 0 {
            self.len -= 1;
            return;
        }
        self.len -= 1;
        if row < self.len {
            let slot = unsafe { self.data.as_ptr().add(row * size) };
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.data.as_ptr().add(self.len * size),
                    slot,
                    size,
                );
            }
        }
    }

    pub fn clear(&mut self) {
        if let Some(drop_fn) = self.drop_item {
            for i in 0..self.len {
                unsafe { drop_fn(self.data.as_ptr().add(i * self.item_layout.size())) };
            }
        }
        self.len = 0;
    }

    #[cold]
    fn grow(&mut self) {
        let new_cap = if self.cap == 0 { 8 } else { self.cap * 2 };
        let new_layout = Self::array_layout(self.item_layout, new_cap);

        let new_ptr = if self.cap == 0 {
            unsafe { alloc(new_layout) }
        } else {
            let old_layout = Self::array_layout(self.item_layout, self.cap);
            unsafe { realloc(self.data.as_ptr(), old_layout, new_layout.size()) }
        };

        self.data = NonNull::new(new_ptr).expect("BlobVec::grow: allocation failed");
        self.cap = new_cap;
    }

    #[inline]
    fn array_layout(item: Layout, count: usize) -> Layout {
        let align = item.align().max(64);
        let mut size = item.size().checked_mul(count).expect("BlobVec: size overflow");
        size = (size + align - 1) & !(align - 1);
        Layout::from_size_align(size, align).expect("BlobVec: invalid layout")
    }
}

impl Drop for BlobVec {
    fn drop(&mut self) {
        if let Some(drop_fn) = self.drop_item {
            for i in 0..self.len {
                unsafe { drop_fn(self.data.as_ptr().add(i * self.item_layout.size())) };
            }
        }
        if self.item_layout.size() > 0 && self.cap > 0 {
            let layout = Self::array_layout(self.item_layout, self.cap);
            unsafe { dealloc(self.data.as_ptr(), layout) };
        }
    }
}

#[repr(align(64))]
pub struct Column {
    pub data: BlobVec,
    pub added_ticks: Vec<u32>,
    pub changed_ticks: Vec<u32>,
    pub last_added_tick: AtomicU32,
    pub last_changed_tick: AtomicU32,
}

impl Column {
    #[inline]
    pub fn new(
        layout: Layout,
        drop_item: Option<unsafe fn(*mut u8)>,
        initial_cap: usize,
    ) -> Self {
        Self {
            data: BlobVec::new(layout, drop_item, initial_cap),
            added_ticks: Vec::with_capacity(initial_cap),
            changed_ticks: Vec::with_capacity(initial_cap),
            last_added_tick: AtomicU32::new(0),
            last_changed_tick: AtomicU32::new(0),
        }
    }

    #[inline]
    pub unsafe fn swap_remove_drop(&mut self, row: usize) {
        unsafe { self.data.swap_remove_drop(row) };
        self.added_ticks.swap_remove(row);
        self.changed_ticks.swap_remove(row);
    }

    #[inline]
    pub unsafe fn swap_remove_forget(
        &mut self,
        row: usize,
        out_ptr: *mut u8,
    ) -> (u32, u32) {
        unsafe { self.data.swap_remove_forget(row, out_ptr) };
        let added = self.added_ticks.swap_remove(row);
        let changed = self.changed_ticks.swap_remove(row);
        (added, changed)
    }

    #[inline]
    pub unsafe fn swap_remove_forget_no_copy(&mut self, row: usize) -> (u32, u32) {
        unsafe { self.data.swap_remove_forget_no_copy(row) };
        let added = self.added_ticks.swap_remove(row);
        let changed = self.changed_ticks.swap_remove(row);
        (added, changed)
    }

    pub fn clear(&mut self) {
        self.data.clear();
        self.added_ticks.clear();
        self.changed_ticks.clear();
    }
}

#[repr(C)]
pub struct Archetype {
    pub id: u32,
    pub last_modified_tick: AtomicU32,
    pub entities: Vec<Entity>,
    pub columns: Box<[UnsafeCell<Column>]>,
    pub signature: Box<[usize]>,
    pub component_to_column: Box<[u32]>,
    pub add_edges: Vec<(usize, u32)>,
    pub remove_edges: Vec<(usize, u32)>,
}

unsafe impl Send for Archetype {}
unsafe impl Sync for Archetype {}
