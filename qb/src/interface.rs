use crate::QBICommunication;

pub trait QBI<T> {
    fn init(cx: T, com: QBICommunication) -> Self;
    fn run(self);
}
