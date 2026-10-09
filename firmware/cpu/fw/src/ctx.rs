use core::cell::Cell;

macro_rules! context_cell {
    ($name:ident) => {
        pub(crate) struct $name<T>(Cell<T>);

        impl<T> $name<T> {
            pub(crate) const fn new(value: T) -> Self {
                Self(Cell::new(value))
            }

            pub(crate) fn set(&self, value: T) {
                self.0.set(value);
            }
        }

        impl<T: Copy> $name<T> {
            pub(crate) fn get(&self) -> T {
                self.0.get()
            }
        }
    };
}

context_cell!(IsrCell);
context_cell!(MainCell);
