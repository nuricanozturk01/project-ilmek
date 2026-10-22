use ilmek_protocol::MessageType;
use ilmek_protocol::wire::{Mode, ServerRole};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Admission {
    Allowed,
    /// A valid request type that this connection mode or server role does not serve.
    WrongContext,
    /// Not something a client may send at all.
    NotARequest,
}

/// Which client-originated messages a connection accepts after HELLO (PROTOCOL.md §2, §11).
pub(super) fn admit(mode: Mode, role: ServerRole, kind: MessageType) -> Admission {
    use MessageType as M;
    let always = matches!(kind, M::Ping | M::Goodbye);
    let allowed = match mode {
        Mode::Enroll => always || kind == M::Enroll,
        Mode::Bulk => always || matches!(kind, M::GetAsset | M::Cancel),
        Mode::Control => {
            always
                || match role {
                    ServerRole::Registry => matches!(
                        kind,
                        M::Resolve
                            | M::ListApps
                            | M::HostRegister
                            | M::HostHeartbeat
                            | M::RegisterApp
                            | M::UnregisterApp
                            | M::UpdateApp
                            | M::GetTrust
                    ),
                    ServerRole::Host => matches!(
                        kind,
                        M::GetManifest | M::AppCall | M::Subscribe | M::EventAck | M::Unsubscribe
                    ),
                }
        }
    };
    let is_request = matches!(
        kind,
        M::Ping
            | M::Goodbye
            | M::Enroll
            | M::Resolve
            | M::ListApps
            | M::HostRegister
            | M::HostHeartbeat
            | M::RegisterApp
            | M::UnregisterApp
            | M::UpdateApp
            | M::GetTrust
            | M::GetManifest
            | M::AppCall
            | M::Subscribe
            | M::EventAck
            | M::Unsubscribe
            | M::GetAsset
            | M::Cancel
    );
    match (is_request, allowed) {
        (false, _) => Admission::NotARequest,
        (true, true) => Admission::Allowed,
        (true, false) => Admission::WrongContext,
    }
}
