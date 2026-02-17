#[macro_export]
macro_rules! add_translator {
    ($broker:expr, $sub_broker:expr, $variant:path, $sub_variant:path, $method:ident) => {
        $broker
            .$method(
                $sub_broker,
                Box::new(|msg| Some($variant(msg))),
                Box::new(|msg| match msg {
                    $sub_variant(ms) => Some(ms),
                    _ => None,
                }),
            )
            .await?
    };
}

#[macro_export]
macro_rules! add_translator_link {
    ($broker:expr, $sub_broker:expr, $in_variant:path, $out_variant:path) => {
        $broker
            .add_translator_link(
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
macro_rules! add_translator_direct {
    ($broker:expr, $sub_broker:expr, $in_variant:path, $out_variant:path) => {
        $broker
            .add_translator_direct(
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
macro_rules! add_translator_o_ti {
    ($broker:expr, $sub_broker:expr, $in_variant:path) => {
        $broker
            .add_translator_o_ti($sub_broker.clone(), Box::new(|msg| Some($in_variant(msg))))
            .await?
    };
}

#[macro_export]
macro_rules! add_translator_o_ti_match {
    ($broker:expr, $sub_broker:expr, $out_variant:path) => {
        $broker
            .add_translator_o_ti(
                $sub_broker.clone(),
                Box::new(|msg| match msg {
                    $out_variant(out) => Some(out),
                    _ => None,
                }),
            )
            .await?
    };
}

#[macro_export]
macro_rules! add_translator_i_ti {
    ($broker:expr, $sub_broker:expr, $in_variant:path) => {
        $broker
            .add_translator_i_ti($sub_broker.clone(), Box::new(|msg| Some($in_variant(msg))))
            .await?
    };
}

#[macro_export]
macro_rules! add_translator_i_ti_match {
    ($broker:expr, $sub_broker:expr, $out_variant:path, $in_variant:path) => {
        $broker
            .add_translator_i_ti(
                $sub_broker.clone(),
                Box::new(|msg| match msg {
                    $out_variant(out) => Some($in_variant(out)),
                    _ => None,
                }),
            )
            .await?
    };
}

#[macro_export]
macro_rules! add_translator_o_to {
    ($broker:expr, $sub_broker:expr, $out_variant:path) => {
        $broker
            .add_translator_o_to($sub_broker.clone(), Box::new(|msg| Some($out_variant(msg))))
            .await?
    };
}

#[macro_export]
macro_rules! add_translator_i_to {
    ($broker:expr, $sub_broker:expr, $out_variant:path) => {
        $broker
            .add_translator_i_to($sub_broker.clone(), Box::new(|msg| Some($out_variant(msg))))
            .await?
    };
}
