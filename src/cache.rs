mod cache {
    // A public struct with a public field of generic type `T`
    pub struct Cache<T> {
        pub cache_http: T,
    }
}