//! Versioned wrappers for [`CoinPayment`](crate::api::CoinPayment) methods.

use crate::v01;
use truapi_macros::versioned_type;

versioned_type! {
    pub enum HostCoinPaymentCreatePurseRequest { V1 => v01::HostCoinPaymentCreatePurseRequest }
    pub enum HostCoinPaymentCreatePurseResponse { V1 => v01::HostCoinPaymentCreatePurseResponse }
    pub enum HostCoinPaymentCreatePurseError { V1 => v01::CoinPaymentError }
    pub enum HostCoinPaymentQueryPurseRequest { V1 => v01::HostCoinPaymentQueryPurseRequest }
    pub enum HostCoinPaymentQueryPurseResponse { V1 => v01::HostCoinPaymentQueryPurseResponse }
    pub enum HostCoinPaymentQueryPurseError { V1 => v01::CoinPaymentError }
    pub enum HostCoinPaymentRebalancePurseRequest { V1 => v01::HostCoinPaymentRebalancePurseRequest }
    pub enum HostCoinPaymentRebalancePurseItem { V1 => v01::CoinPaymentStatus }
    pub enum HostCoinPaymentRebalancePurseError { V1 => v01::CoinPaymentError }
    pub enum HostCoinPaymentDeletePurseRequest { V1 => v01::HostCoinPaymentDeletePurseRequest }
    pub enum HostCoinPaymentDeletePurseItem { V1 => v01::CoinPaymentStatus }
    pub enum HostCoinPaymentDeletePurseError { V1 => v01::CoinPaymentError }
    pub enum HostCoinPaymentCreateReceivableRequest { V1 => v01::HostCoinPaymentCreateReceivableRequest }
    pub enum HostCoinPaymentCreateReceivableResponse { V1 => v01::HostCoinPaymentCreateReceivableResponse }
    pub enum HostCoinPaymentCreateReceivableError { V1 => v01::CoinPaymentError }
    pub enum HostCoinPaymentCreateChequeRequest { V1 => v01::HostCoinPaymentCreateChequeRequest }
    pub enum HostCoinPaymentCreateChequeResponse { V1 => v01::HostCoinPaymentCreateChequeResponse }
    pub enum HostCoinPaymentCreateChequeError { V1 => v01::CoinPaymentError }
    pub enum HostCoinPaymentDepositRequest { V1 => v01::HostCoinPaymentDepositRequest }
    pub enum HostCoinPaymentDepositItem { V1 => v01::CoinPaymentStatus }
    pub enum HostCoinPaymentDepositError { V1 => v01::CoinPaymentError }
    pub enum HostCoinPaymentRefundRequest { V1 => v01::HostCoinPaymentRefundRequest }
    pub enum HostCoinPaymentRefundItem { V1 => v01::CoinPaymentStatus }
    pub enum HostCoinPaymentRefundError { V1 => v01::CoinPaymentError }
    pub enum HostCoinPaymentListenForRequest { V1 => v01::HostCoinPaymentListenForRequest }
    pub enum HostCoinPaymentListenForItem { V1 => v01::HostCoinPaymentListenForItem }
    pub enum HostCoinPaymentListenForError { V1 => v01::CoinPaymentError }
}
