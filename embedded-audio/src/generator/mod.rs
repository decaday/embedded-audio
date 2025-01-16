pub mod sine_wave;
pub use sine_wave::SineWaveGenerator;

#[macro_export]
macro_rules! impl_element_for_reader_element {
    // Handle types with generics and trait bounds
    ($type:ident<$($gen:tt),*> where $($bound:tt)+) => {
        impl<$($gen),*> embedded_audio_driver::element::Element<embedded_audio_driver::element::ReaderMode> for $type<$($gen),*>
        where
            $($bound)+
        {
            type Error = core::convert::Infallible;
            fn get_out_info(&self) -> Option<embedded_audio_driver::info::Info> {
                Some(self.get_info())
            }

            fn get_in_info(&self) -> Option<embedded_audio_driver::info::Info> {
                None
            }
        }
    };

    // Handle types with generics but no trait bounds
    ($type:ident<$($gen:tt),*>) => {
        impl<$($gen),*> embedded_audio_driver::element::Element<embedded_audio_driver::element::ReaderMode> for $type<$($gen),*> {
            type Error = core::convert::Infallible;
            
            fn get_out_info(&self) -> Option<embedded_audio_driver::info::Info> {
                Some(self.get_info())
            }

            fn get_in_info(&self) -> Option<embedded_audio_driver::info::Info> {
                None
            }
        }
    };

    // Handle types without generics
    ($type:ty) => {
        impl embedded_audio_driver::element::Element for $type {
            type Error = core::convert::Infallible;

            fn get_out_info(&self) -> Option<embedded_audio_driver::info::Info> {
                Some(self.get_info())
            }

            fn get_in_info(&self) -> Option<embedded_audio_driver::info::Info> {
                None
            }

            fn process<PR, PW>(&mut self, _reader: Option<PR>, _writer: Option<PW>) -> Result<(),Self::Error> {
                Ok(())
            }
        }
    };
}

#[macro_export]
macro_rules! impl_read_for_reader_element {
    // Common implementation for ErrorType
    (@impl_error_type $type:ty) => {
        impl embedded_io::ErrorType for $type {
            type Error = core::convert::Infallible;
        }
    };

    // Common implementation for Read
    (@impl_read $type:ty) => {
        impl embedded_io::Read for $type {
            fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
                embedded_audio_driver::element::ReaderElement::read(self, buf)
            }
        }
    };

    // Handle types with generics and trait bounds
    ($type:ident<$($gen:tt),*> where $($bound:tt)+) => {
        impl<$($gen),*> embedded_io::ErrorType for $type<$($gen),*>
        where
            $($bound)+
        {
            type Error = core::convert::Infallible;
        }

        impl<$($gen),*> embedded_io::Read for $type<$($gen),*>
        where
            $($bound)+
        {
            fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
                embedded_audio_driver::element::ReaderElement::read(self, buf)
            }
        }
    };

    // Handle types with generics but no trait bounds
    ($type:ident<$($gen:tt),*>) => {
        impl_read_for_reader_element!(@impl_error_type $type<$($gen),*>);
        impl_read_for_reader_element!(@impl_read $type<$($gen),*>);
    };

    // Handle types without generics
    ($type:ty) => {
        impl_read_for_reader_element!(@impl_error_type $type);
        impl_read_for_reader_element!(@impl_read $type);
    };
}