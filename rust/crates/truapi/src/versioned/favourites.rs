//! Versioned wrappers for [`Favourites`](crate::api::Favourites) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostFavouritesSubscribeItem { V1 => v01::HostFavouritesSubscribeItem }
}

truapi_macros::versioned_type! {
    pub enum HostFavouritesSubscribeError { V1 => v01::HostFavouritesSubscribeError }
}

truapi_macros::versioned_type! {
    pub enum HostFavouritesAddRequest { V1 => v01::HostFavouritesAddRequest }
}

truapi_macros::versioned_type! {
    pub enum HostFavouritesAddResponse { V1 => v01::HostFavouritesAddResponse }
}

truapi_macros::versioned_type! {
    pub enum HostFavouritesAddError { V1 => v01::HostFavouritesAddError }
}

truapi_macros::versioned_type! {
    pub enum HostFavouritesForgetRequest { V1 => v01::HostFavouritesForgetRequest }
}

truapi_macros::versioned_type! {
    pub enum HostFavouritesForgetError { V1 => v01::HostFavouritesForgetError }
}
