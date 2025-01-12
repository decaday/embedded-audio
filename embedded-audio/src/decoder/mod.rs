pub mod wav;
pub mod reader;

#[macro_export]
macro_rules! impl_element_for_decoder {
    // Handle types with generics and trait bounds
    ($type:ident<$($gen:tt),*> where $($bound:tt)+) => {
        impl<$($gen),*> embedded_audio_driver::element::Element for $type<$($gen),*>
        where
            $($bound)+
        {
            fn get_out_info(&self) -> Option<embedded_audio_driver::element::Info> {
                Some(self.get_info())
            }

            fn get_in_info(&self) -> Option<embedded_audio_driver::element::Info> {
                None
            }
        }
    };

    // Handle types with generics but no trait bounds
    ($type:ident<$($gen:tt),*>) => {
        impl<$($gen),*> embedded_audio_driver::element::Element for $type<$($gen),*> {
            fn get_out_info(&self) -> Option<embedded_audio_driver::element::Info> {
                Some(self.get_info())
            }

            fn get_in_info(&self) -> Option<embedded_audio_driver::element::Info> {
                None
            }
        }
    };

    // Handle types without generics
    ($type:ty) => {
        impl embedded_audio_driver::element::Element for $type {
            fn get_out_info(&self) -> Option<embedded_audio_driver::element::Info> {
                Some(self.get_info())
            }

            fn get_in_info(&self) -> Option<embedded_audio_driver::element::Info> {
                None
            }
        }
    };
}
