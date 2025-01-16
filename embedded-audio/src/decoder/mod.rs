mod wav;
pub use wav::WavDecoder;

#[macro_export]
macro_rules! impl_element_for_decoder {
    // Handle types with generics and trait bounds
    ($type:ident<$($gen:tt),*> where $($bound:tt)+) => {
        impl<$($gen),*> embedded_audio_driver::element::Element for $type<$($gen),*>
        where
            $($bound)+
        {
            impl_element_for_decoder!(@impl_body);
        }
    };

    // Handle types with generics but no trait bounds
    ($type:ident<$($gen:tt),*>) => {
        impl<$($gen),*> embedded_audio_driver::element::Element for $type<$($gen),*> {
            impl_element_for_decoder!(@impl_body);
        }
    };

    // Handle types without generics
    ($type:ty) => {
        impl embedded_audio_driver::element::Element for $type {
            impl_element_for_decoder!(@impl_body);
        }
    };

    // Common implementation body
    (@impl_body) => {
        type Error = embedded_audio_driver::decoder::Error;

        fn get_out_info(&self) -> Option<embedded_audio_driver::info::Info> {
            Some(self.get_info())
        }

        fn get_in_info(&self) -> Option<embedded_audio_driver::info::Info> {
            None
        }

        fn process<PR, PW>(&mut self, _reader: Option<PR>, _writer: Option<PW>) -> Result<(), Self::Error> {
            Ok(())
        }
    };
}

#[macro_export]
macro_rules! impl_read_for_decoder {
    // Common implementation for ErrorType
    (@impl_error_type) => {
        type Error = embedded_audio_driver::decoder::Error;
    };

    // Common implementation for Read
    (@impl_read) => {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
            embedded_audio_driver::decoder::Decoder::read(self, buf)
        }
    };

    // Handle types with generics and trait bounds
    ($type:ident<$($gen:tt),*> where $($bound:tt)+) => {
        impl<$($gen),*> embedded_io::ErrorType for $type<$($gen),*>
        where
            $($bound)+
        {
            impl_read_for_decoder!(@impl_error_type);
        }

        impl<$($gen),*> embedded_io::Read for $type<$($gen),*>
        where
            $($bound)+
        {
            impl_read_for_decoder!(@impl_read);
        }
    };

    // Handle types with generics but no trait bounds
    ($type:ident<$($gen:tt),*>) => {
        impl<$($gen),*> embedded_io::ErrorType for $type<$($gen),*> {
            impl_read_for_decoder!(@impl_error_type);
        }

        impl<$($gen),*> embedded_io::Read for $type<$($gen),*> {
            impl_read_for_decoder!(@impl_read $type<$($gen),*>);
        }
    };

    // Handle types without generics
    ($type:ty) => {
        impl embedded_io::ErrorType for $type {
            impl_read_for_decoder!(@impl_error_type);
        }

        impl embedded_io::Read for $type {
            impl_read_for_decoder!(@impl_read $type);
        }
    };
}

