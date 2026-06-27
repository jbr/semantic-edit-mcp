mod m {
    /// Alpha.
    fn alpha<T: Clone>(x: T) -> T
    where
        T: Default,
    {
        x
    }

    /// Beta.
    fn beta() {}
}
