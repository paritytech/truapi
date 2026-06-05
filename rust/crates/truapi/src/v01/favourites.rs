use parity_scale_codec::{Decode, Encode};

/// How a product entered the favourites catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum FavouriteProductSource {
    /// Discovered via the on-chain registry.
    Remote,
    /// Sideloaded or manually added.
    Local,
}

/// A bookmarked product in the host's local catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct FavouriteProduct {
    /// DotNS identifier of the product.
    pub product_id: String,
    /// How the product was added.
    pub source: FavouriteProductSource,
    /// Unix timestamp (seconds) when first bookmarked.
    pub created_at: u64,
    /// Unix timestamp (seconds) of the most recent update.
    pub updated_at: u64,
}

/// Request to add a product to the favourites catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFavouritesAddRequest {
    /// DotNS identifier of the product to add.
    pub product_id: String,
}

/// Response after adding a product to favourites.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFavouritesAddResponse {
    /// The resulting catalogue entry.
    pub product: FavouriteProduct,
}

/// Request to remove a product from the favourites catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFavouritesForgetRequest {
    /// DotNS identifier of the product to remove.
    pub product_id: String,
}

/// Error from [`crate::api::Favourites::subscribe`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFavouritesSubscribeError {
    /// Catch-all.
    Unknown { reason: String },
}

/// Error from [`crate::api::Favourites::add`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFavouritesAddError {
    /// Catch-all.
    Unknown { reason: String },
}

/// Error from [`crate::api::Favourites::forget`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFavouritesForgetError {
    /// The product was not in the catalogue.
    NotFound,
    /// Catch-all.
    Unknown { reason: String },
}

/// Item pushed to favourites subscribers: the full list of bookmarked products.
pub type HostFavouritesSubscribeItem = Vec<FavouriteProduct>;
