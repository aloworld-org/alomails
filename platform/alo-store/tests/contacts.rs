//! Address-book store: contact CRUD, that writes advance the account
//! modseq (JMAP/CardDAV change tracking), vCard round-tripping through
//! the store model, and that contacts are per-account — one user never
//! sees or touches another's.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common::{fresh_account, test_store};
use alo_store::vcard;
use alo_store::{Contact, ContactField, ContactId, StoreError};

fn sample(name: &str, email: &str) -> Contact {
    Contact {
        id: ContactId::new(""), // set by the store on create
        display_name: name.to_owned(),
        first_name: None,
        last_name: None,
        emails: vec![ContactField {
            kind: Some("work".to_owned()),
            value: email.to_owned(),
        }],
        phones: Vec::new(),
        organization: None,
        job_title: None,
        notes: None,
    }
}

#[tokio::test]
async fn create_list_get_update_delete() {
    let store = test_store().await;
    let (acc, _u, _inbox) = fresh_account(&store, "contact-crud").await;

    let before = acc.state().await.unwrap();
    let id = acc
        .create_contact(&sample("Ada Lovelace", "ada@analytical.eng"))
        .await
        .unwrap();
    // A write advances the account modseq (so /changes and sync see it).
    assert_ne!(
        acc.state().await.unwrap(),
        before,
        "create bumps the state token"
    );

    let list = acc.contacts().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, id);
    assert_eq!(list[0].display_name, "Ada Lovelace");
    assert_eq!(list[0].emails[0].value, "ada@analytical.eng");
    assert_eq!(list[0].emails[0].kind.as_deref(), Some("work"));

    // get by id.
    let got = acc.contact(&id).await.unwrap().expect("contact exists");
    assert_eq!(got.display_name, "Ada Lovelace");

    // update.
    let mut edited = got.clone();
    edited.organization = Some("Analytical Engine Co".to_owned());
    edited.phones = vec![ContactField {
        kind: None,
        value: "+44 20 7946 0000".to_owned(),
    }];
    acc.update_contact(&id, &edited).await.unwrap();
    let got = acc.contact(&id).await.unwrap().unwrap();
    assert_eq!(got.organization.as_deref(), Some("Analytical Engine Co"));
    assert_eq!(got.phones[0].value, "+44 20 7946 0000");

    // delete.
    acc.delete_contact(&id).await.unwrap();
    assert!(acc.contact(&id).await.unwrap().is_none());
    assert!(acc.contacts().await.unwrap().is_empty());
}

#[tokio::test]
async fn update_and_delete_reject_unknown_ids() {
    let store = test_store().await;
    let (acc, _u, _inbox) = fresh_account(&store, "contact-missing").await;
    let ghost = ContactId::new("does-not-exist");
    assert!(matches!(
        acc.update_contact(&ghost, &sample("X", "x@y.eu")).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        acc.delete_contact(&ghost).await,
        Err(StoreError::NotFound)
    ));
}

#[tokio::test]
async fn stored_contact_round_trips_through_vcard() {
    let store = test_store().await;
    let (acc, _u, _inbox) = fresh_account(&store, "contact-vcard").await;
    let mut c = sample("Katherine Johnson", "katherine@nasa.gov");
    c.first_name = Some("Katherine".to_owned());
    c.last_name = Some("Johnson".to_owned());
    c.notes = Some("Orbital mechanics; hidden figure.".to_owned());
    let id = acc.create_contact(&c).await.unwrap();

    let stored = acc.contact(&id).await.unwrap().unwrap();
    // The exact stored contact serializes and parses back byte-for-value.
    let vcard = vcard::to_vcard(&stored);
    assert!(vcard.contains("FN:Katherine Johnson"));
    let parsed = vcard::from_vcard(&vcard).unwrap();
    assert_eq!(
        parsed, stored,
        "store→vCard→store is lossless for our fields"
    );
}

#[tokio::test]
async fn contacts_are_per_account() {
    // The mandatory wrong-tenant test at the store layer: Bob's door can
    // never read, update, or delete a contact created through Alice's.
    let store = test_store().await;
    let (alice, _ua, _ia) = fresh_account(&store, "contact-alice").await;
    let (bob, _ub, _ib) = fresh_account(&store, "contact-bob").await;

    let id = alice
        .create_contact(&sample("Alice Only", "secret@alice.eu"))
        .await
        .unwrap();

    // Bob sees nothing and cannot fetch Alice's id.
    assert!(bob.contacts().await.unwrap().is_empty());
    assert!(bob.contact(&id).await.unwrap().is_none());

    // Bob's update/delete of Alice's id are clean NotFound denials...
    assert!(matches!(
        bob.update_contact(&id, &sample("Tampered", "t@b.eu")).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        bob.delete_contact(&id).await,
        Err(StoreError::NotFound)
    ));

    // ...and Alice's contact is intact.
    let still = alice.contact(&id).await.unwrap().unwrap();
    assert_eq!(still.display_name, "Alice Only");
}
