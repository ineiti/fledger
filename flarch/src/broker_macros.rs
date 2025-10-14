#[macro_export]
macro_rules! add_translator {
    ($broker:expr, $sub_broker:expr, $out_variant:path, $in_variant:path, $method:ident) => {
        $broker
            .$method(
                $sub_broker,
                Box::new(|msg| match msg {
                    $out_variant(ms) => Some(ms),
                    _ => None,
                }),
                Box::new(|msg| Some($in_variant(msg))),
            )
            .await?
    };
}

#[macro_export]
macro_rules! add_translator_link {
    ($broker:expr, $sub_broker:expr, $out_variant:path, $in_variant:path) => {
        flarch::add_translator!(
            $broker,
            $sub_broker,
            $out_variant,
            $in_variant,
            add_translator_link
        );
    };
}

#[macro_export]
macro_rules! add_translator_direct {
    ($broker:expr, $sub_broker:expr, $out_variant:path, $in_variant:path) => {
        flarch::add_translator!(
            $broker,
            $sub_broker,
            $out_variant,
            $in_variant,
            add_translator_direct
        );
    };
}
