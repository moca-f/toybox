use hibitset::{BitIter, BitSetAnd, BitSetLike};

use tb_core::Id;

pub trait Join: Sized {
    type BitSet: BitSetLike;
    type Component;
    type Components;
    fn join(self) -> JoinIterator<Self> {
        let (mask, components) = self.open();
        JoinIterator {
            mask_iter: mask.iter(),
            components,
        }
    }
    fn open(self) -> (Self::BitSet, Self::Components);

    /// # Safety
    ///
    /// There must be component associated with given `Id`.
    unsafe fn get(components: &mut Self::Components, id: Id) -> Self::Component;
}

pub struct JoinIterator<J: Join> {
    mask_iter: BitIter<J::BitSet>,
    components: J::Components,
}

impl<J: Join> Iterator for JoinIterator<J> {
    type Item = J::Component;

    fn next(&mut self) -> Option<Self::Item> {
        self.mask_iter
            .next()
            .map(|id| unsafe { J::get(&mut self.components, id.into()) })
    }
}

macro_rules! bit_set_and {
    ($b:ty) => { $b };
    ($b0:ty, $($b1:ty), +) => {
        BitSetAnd<$b0, bit_set_and!($($b1), +)>
    };
    ($b:expr) => { $b };
    ($b0:expr, $($b1:expr), +) => {
        BitSetAnd($b0, bit_set_and!($($b1), +))
    };
}

macro_rules! impl_join_tuple {
    ($j:ident) => {};
    ($j0:ident, $($j1:ident), +) => {
        impl_join_tuple!($($j1), +);
        impl<$j0: Join, $($j1: Join), +> Join for ($j0, $($j1), +) {
            type BitSet = bit_set_and!($j0::BitSet, $($j1::BitSet), +);
            type Component = ($j0::Component, $($j1::Component), +);
            type Components = ($j0::Components, $($j1::Components), +);

            #[allow(non_snake_case)]
            fn open(self) -> (Self::BitSet, Self::Components) {
                let ($j0, $($j1), +) = self;
                let ($j0, $($j1), +) = ($j0.open(), $($j1.open()), +);
                (bit_set_and!($j0.0, $($j1.0), +), ($j0.1, $($j1.1), +))
            }

            #[allow(non_snake_case)]
            unsafe fn get(components: &mut Self::Components, id: Id) -> Self::Component {
                let ($j0, $($j1), +) = components;
                ($j0::get($j0, id), $($j1::get($j1, id)), +)
            }
        }
    };
}

impl_join_tuple!(J0, J1, J2, J3, J4, J5, J6, J7);
