pub mod wav;
pub mod writer;

#[macro_export]
macro_rules! impl_element_for_encoder {
    // Handle types with generics and trait bounds
    ($type:ident<$($gen:tt),*> where $($bound:tt)+) => {
        impl<$($gen),*> embedded_audio_driver::element::Element for $type<$($gen),*>
        where
            $($bound)+
        {
            impl_element_for_encoder!(@impl_body);
        }
    };

    // Handle types with generics but no trait bounds
    ($type:ident<$($gen:tt),*>) => {
        impl<$($gen),*> embedded_audio_driver::element::Element for $type<$($gen),*> {
            impl_element_for_encoder!(@impl_body);
        }
    };

    // Handle types without generics
    ($type:ty) => {
        impl embedded_audio_driver::element::Element for $type {
            impl_element_for_encoder!(@impl_body);
        }
    };

    // Common implementation body
    (@impl_body) => {
        type Error = embedded_audio_driver::encoder::Error;

        fn get_out_info(&self) -> Option<embedded_audio_driver::info::Info> {
            None
        }

        fn get_in_info(&self) -> Option<embedded_audio_driver::info::Info> {
            Some(self.get_info())
        }
    };
}
