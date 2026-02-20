#[macro_export]
macro_rules! add_translator {
    // Internal helper macro to dispatch to the correct method
    (@call_method o_ti, $broker_src:expr, $broker_dst:expr, $translator:expr) => {
        $broker_src
            .add_translator_o_ti($broker_dst, $translator)
            .await?
    };
    (@call_method i_ti, $broker_src:expr, $broker_dst:expr, $translator:expr) => {
        $broker_src
            .add_translator_i_ti($broker_dst, $translator)
            .await?
    };
    (@call_method o_to, $broker_src:expr, $broker_dst:expr, $translator:expr) => {
        $broker_src
            .add_translator_o_to($broker_dst, $translator)
            .await?
    };
    (@call_method i_to, $broker_src:expr, $broker_dst:expr, $translator:expr) => {
        $broker_src
            .add_translator_i_to($broker_dst, $translator)
            .await?
    };

    ($broker_src:expr, $variant:ident, $broker_dst:expr) => {
        add_translator!(@call_method $variant, $broker_src, $broker_dst.clone(),
            Box::new(|msg| Some(msg))
        )
    };

    ($broker_src:expr, $variant:ident, $broker_dst:expr, $pattern_in:pat => $pattern_out:expr) => {
        add_translator!(@call_method $variant, $broker_src, $broker_dst.clone(),
                Box::new(|msg| match msg {
                    $pattern_in => Some($pattern_out),
                    _ => None,
                })
            )
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
    ($broker:expr, $sub_broker:expr, $out_match:path, $in_variant:path) => {
        $broker
            .add_translator_o_ti(
                $sub_broker.clone(),
                Box::new(|msg| match msg {
                    $out_match(out) => Some($in_variant(out)),
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
    ($broker:expr, $sub_broker:expr, $out_match:path, $in_variant:path) => {
        $broker
            .add_translator_i_ti(
                $sub_broker.clone(),
                Box::new(|msg| match msg {
                    $out_match(out) => Some($in_variant(out)),
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
macro_rules! add_translator_o_to_match {
    ($broker:expr, $sub_broker:expr, $out_match:path, $out_variant:path) => {
        $broker
            .add_translator_o_to(
                $sub_broker.clone(),
                Box::new(|msg| match msg {
                    $out_match(out) => Some($out_variant(out)),
                    _ => None,
                }),
            )
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

#[macro_export]
macro_rules! add_translator_i_to_match {
    ($broker:expr, $sub_broker:expr, $in_match:path, $out_variant:path) => {
        $broker
            .add_translator_i_to($sub_broker.clone(), Box::new(|msg| match msg {
                $in_match(in) => Some($out_variant(in)),
                _ => None,
            }))
            .await?
    };
}
