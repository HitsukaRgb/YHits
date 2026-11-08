#[macro_export]
macro_rules! define_tr {
    ($(($key:ident, $ru:expr, $en:expr)),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Tr {
            $($key),*
        }

        impl Tr {
            pub fn get(self, lang: $crate::i18n::Language) -> &'static str {
                match lang {
                    $crate::i18n::Language::Russian => match self {
                        $(Self::$key => $ru,)*
                    },
                    $crate::i18n::Language::English => match self {
                        $(Self::$key => $en,)*
                    },
                }
            }
        }

        #[inline]
        pub fn t(key: Tr) -> &'static str {
            key.get($crate::i18n::current_language())
        }
    };
}
