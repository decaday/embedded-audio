mod wav;
pub use wav::WavEncoder;

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
            Some(embedded_audio_driver::encoder::Encoder::get_info(self))
        }

        fn process<PR, PW>(&mut self, _reader: Option<PR>, _writer: Option<PW>) -> Result<(), embedded_audio_driver::encoder::Error> {
            Ok(())
        }
    };
}

#[macro_export]
macro_rules! impl_write_for_encoder {
    // Common implementation for ErrorType
    (@impl_error_type) => {
        type Error = embedded_audio_driver::encoder::Error;
    };

    // Common implementation for Write
    (@impl_write) => {
        fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
            embedded_audio_driver::encoder::Encoder::write(self, buf)
        }

        fn flush(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    };

    // Handle types with generics and trait bounds
    ($type:ident<$($gen:tt),*> where $($bound:tt)+) => {
        impl<$($gen),*> embedded_io::ErrorType for $type<$($gen),*>
        where
            $($bound)+
        {
            impl_write_for_encoder!(@impl_error_type);
        }

        impl<$($gen),*> embedded_io::Write for $type<$($gen),*>
        where
            $($bound)+
        {
            impl_write_for_encoder!(@impl_write);
        }
    };

    // Handle types with generics but no trait bounds
    ($type:ident<$($gen:tt),*>) => {
        impl<$($gen),*> embedded_io::ErrorType for $type<$($gen),*> {
            impl_write_for_encoder!(@impl_error_type);
        }

        impl<$($gen),*> embedded_io::Write for $type<$($gen),*> {
            impl_write_for_encoder!(@impl_write);
        }
    };

    // Handle types without generics
    ($type:ty) => {
        impl embedded_io::ErrorType for $type {
            impl_write_for_encoder!(@impl_error_type);
        }

        impl embedded_io::Write for $type {
            impl_write_for_encoder!(@impl_write);
        }
    };
}