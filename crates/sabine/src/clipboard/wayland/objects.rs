use std::{os::fd::BorrowedFd, sync::Mutex};

use wayland_client::{
    Proxy,
    protocol::{wl_data_offer::WlDataOffer, wl_data_source::WlDataSource},
};
use wayland_protocols::{
    ext::data_control::v1::client::{
        ext_data_control_offer_v1::ExtDataControlOfferV1,
        ext_data_control_source_v1::ExtDataControlSourceV1,
    },
    wp::primary_selection::zv1::client::{
        zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1,
        zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1,
    },
};

use crate::clipboard::ClipboardContent;

/// The MIME types an offer has announced so far.
#[derive(Default)]
pub(super) struct OfferTypes(Mutex<Vec<String>>);

impl OfferTypes {
    pub(super) fn add(&self, mime: String) {
        self.0.lock().unwrap().push(mime);
    }

    pub(super) fn get(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Offer {
    Data(WlDataOffer),
    Primary(ZwpPrimarySelectionOfferV1),
    Control(ExtDataControlOfferV1),
}

impl Offer {
    pub(super) fn types(&self) -> Vec<String> {
        let types = match self {
            Self::Data(offer) => offer.data::<OfferTypes>(),
            Self::Primary(offer) => offer.data::<OfferTypes>(),
            Self::Control(offer) => offer.data::<OfferTypes>(),
        };
        types.map(OfferTypes::get).unwrap_or_default()
    }

    pub(super) fn receive(&self, mime: String, fd: BorrowedFd<'_>) {
        match self {
            Self::Data(offer) => offer.receive(mime, fd),
            Self::Primary(offer) => offer.receive(mime, fd),
            Self::Control(offer) => offer.receive(mime, fd),
        }
    }

    pub(super) fn destroy(&self) {
        match self {
            Self::Data(offer) => offer.destroy(),
            Self::Primary(offer) => offer.destroy(),
            Self::Control(offer) => offer.destroy(),
        }
    }
}

pub(super) enum Source {
    Data(WlDataSource),
    Primary(ZwpPrimarySelectionSourceV1),
    Control(ExtDataControlSourceV1),
}

impl Source {
    pub(super) fn offer_types(&self, content: &ClipboardContent) {
        for mime in content.offered_types() {
            match self {
                Self::Data(source) => source.offer(mime),
                Self::Primary(source) => source.offer(mime),
                Self::Control(source) => source.offer(mime),
            }
        }
    }

    pub(super) fn destroy(&self) {
        match self {
            Self::Data(source) => source.destroy(),
            Self::Primary(source) => source.destroy(),
            Self::Control(source) => source.destroy(),
        }
    }

    pub(super) fn is(&self, other: &impl Proxy) -> bool {
        let id = match self {
            Self::Data(source) => source.id(),
            Self::Primary(source) => source.id(),
            Self::Control(source) => source.id(),
        };
        id == other.id()
    }
}
