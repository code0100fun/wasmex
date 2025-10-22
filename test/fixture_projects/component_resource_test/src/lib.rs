#[allow(warnings)]
mod bindings;

use bindings::exports::component::resource_test::types::{Counter, Guest, GuestCounter};
use std::cell::Cell;

struct Component;

// Counter resource implementation using Cell for interior mutability
struct CounterImpl {
    value: Cell<u32>,
}

impl GuestCounter for CounterImpl {
    fn new(initial: u32) -> Self {
        CounterImpl {
            value: Cell::new(initial),
        }
    }

    fn increment(&self) -> u32 {
        let new_value = self.value.get() + 1;
        self.value.set(new_value);
        new_value
    }

    fn get(&self) -> u32 {
        self.value.get()
    }

    fn add(&self, amount: u32) -> u32 {
        let new_value = self.value.get() + amount;
        self.value.set(new_value);
        new_value
    }
}

impl Guest for Component {
    type Counter = CounterImpl;

    fn create_counter(initial: u32) -> Counter {
        Counter::new(CounterImpl::new(initial))
    }

    fn use_counter(c: Counter, amount: u32) -> u32 {
        c.get::<CounterImpl>().add(amount)
    }
}

bindings::export!(Component with_types_in bindings);
