//! Host controls for falsifying the lag-one overlap and association proofs.
#[derive(Clone, Copy, Default)]
pub(super) struct Controls {
    pub serial: bool,
    pub wrong_association: Option<u64>,
    pub draw_error: Option<u64>,
    pub quit: Option<u64>,
    #[cfg(not(target_os = "vita"))]
    order: Option<Order>,
}

#[cfg(not(target_os = "vita"))]
#[derive(Clone, Copy)]
enum Order {
    Rendezvous,
    OwnerFirst,
    WorkerFirst,
}

impl Controls {
    pub(super) fn read() -> anyhow::Result<Self> {
        #[cfg(not(target_os = "vita"))]
        {
            let order = match std::env::var("LOOKING_GLASS_SIM_LAG_ORDER").as_deref() {
                Err(std::env::VarError::NotPresent) => None,
                Ok("rendezvous") => Some(Order::Rendezvous),
                Ok("owner-first") => Some(Order::OwnerFirst),
                Ok("worker-first") => Some(Order::WorkerFirst),
                _ => anyhow::bail!("invalid lag-one execution order"),
            };
            fn frame(name: &str) -> anyhow::Result<Option<u64>> {
                Ok(std::env::var(name).ok().map(|s| s.parse()).transpose()?)
            }
            Ok(Self {
                serial: std::env::var_os("LOOKING_GLASS_SIM_LAG_SERIAL").is_some(),
                wrong_association: frame("LOOKING_GLASS_SIM_LAG_WRONG_ASSOCIATION")?,
                draw_error: frame("LOOKING_GLASS_SIM_LAG_DRAW_ERROR")?,
                quit: frame("LOOKING_GLASS_SIM_LAG_QUIT")?,
                order,
            })
        }
        #[cfg(target_os = "vita")]
        Ok(Self::default())
    }

    pub(super) fn pair(self, pending: bool) -> (Option<Peer>, Option<Peer>) {
        #[cfg(not(target_os = "vita"))]
        if let Some(order) = self.order.filter(|_| pending) {
            use std::sync::mpsc::sync_channel;
            let (owner_tx, owner_rx) = sync_channel(1);
            let (worker_tx, worker_rx) = sync_channel(1);
            return (
                Some(Peer {
                    tx: owner_tx,
                    rx: worker_rx,
                    first: matches!(order, Order::OwnerFirst),
                    second: matches!(order, Order::WorkerFirst),
                }),
                Some(Peer {
                    tx: worker_tx,
                    rx: owner_rx,
                    first: matches!(order, Order::WorkerFirst),
                    second: matches!(order, Order::OwnerFirst),
                }),
            );
        }
        let _ = pending;
        (None, None)
    }
}

pub(super) struct Peer {
    #[cfg(not(target_os = "vita"))]
    tx: std::sync::mpsc::SyncSender<()>,
    #[cfg(not(target_os = "vita"))]
    rx: std::sync::mpsc::Receiver<()>,
    #[cfg(not(target_os = "vita"))]
    first: bool,
    #[cfg(not(target_os = "vita"))]
    second: bool,
}

impl Peer {
    pub(super) fn enter(self) {
        #[cfg(not(target_os = "vita"))]
        {
            if !self.second {
                self.tx.send(()).expect("missing overlap peer");
            }
            if !self.first {
                self.rx.recv().expect("missing overlap peer");
            }
            if self.second {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    }
}
