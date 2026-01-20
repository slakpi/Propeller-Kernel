//! Basic Low-Level Module Testing Utilities
//!
//!   TODO: Re-enable printing.

pub struct TestContext {
  pub pass_count: u32,
  pub fail_count: u32,
}

impl TestContext {
  pub fn new() -> Self {
    TestContext {
      pass_count: 0,
      fail_count: 0,
    }
  }
}

#[macro_export]
macro_rules! execute_test {
  ($ctx:ident, $fn:ident) => {
    // let mut context = crate::test::TestContext::new();
    // debug_print!("* {}:\n", stringify!($fn));
    $fn($ctx);
    // debug_print!(
    //   "  Pass: {}, Fail: {}\n",
    //   context.pass_count,
    //   context.fail_count
    // );
  };
}

#[macro_export]
macro_rules! check_eq {
  ($ctx:ident, $act:expr, $exp:expr) => {
    if $act != $exp {
      $ctx.fail_count += 1;
      // debug_print!("    FAIL: {} != {} ({} {})\n", $act, $exp, file!(), line!());
    } else {
      $ctx.pass_count += 1;
    }
  };
}

#[macro_export]
macro_rules! check_neq {
  ($ctx:ident, $act:expr, $exp:expr) => {
    if $act == $exp {
      $ctx.fail_count += 1;
      // debug_print!("    FAIL: {} == {} ({} {})\n", $act, $exp, file!(), line!());
    } else {
      $ctx.pass_count += 1;
    }
  };
}

#[macro_export]
macro_rules! check_lt {
  ($ctx:ident, $act:expr, $exp:expr) => {
    if $act >= $exp {
      $ctx.fail_count += 1;
      // debug_print!("    FAIL: {} >= {} ({} {})\n", $act, $exp, file!(), line!());
    } else {
      $ctx.pass_count += 1;
    }
  };
}

#[macro_export]
macro_rules! check_lteq {
  ($ctx:ident, $act:expr, $exp:expr) => {
    if $act > $exp {
      $ctx.fail_count += 1;
      // debug_print!("    FAIL: {} > {} ({} {})\n", $act, $exp, file!(), line!());
    } else {
      $ctx.pass_count += 1;
    }
  };
}

#[macro_export]
macro_rules! check_gt {
  ($ctx:ident, $act:expr, $exp:expr) => {
    if $act <= $exp {
      $ctx.fail_count += 1;
      // debug_print!("    FAIL: {} <= {} ({} {})\n", $act, $exp, file!(), line!());
    } else {
      $ctx.pass_count += 1;
    }
  };
}

#[macro_export]
macro_rules! check_gteq {
  ($ctx:ident, $act:expr, $exp:expr) => {
    if $act < $exp {
      $ctx.fail_count += 1;
      // debug_print!("    FAIL: {} < {} ({} {})\n", $act, $exp, file!(), line!());
    } else {
      $ctx.pass_count += 1;
    }
  };
}

#[macro_export]
macro_rules! check_not_none {
  ($ctx:ident, $act:expr) => {
    if $act.is_none() {
      $ctx.fail_count += 1;
      // debug_print!(
      //   "   FAIL: {} is None ({} {})\n",
      //   stringify!($act),
      //   file!(),
      //   line!()
      // );
    } else {
      $ctx.pass_count += 1;
    }
  };
}

#[macro_export]
macro_rules! check_none {
  ($ctx:ident, $act:expr) => {
    if !$act.is_none() {
      $ctx.fail_count += 1;
      // debug_print!(
      //   "   FAIL: {} is not None ({} {})\n",
      //   stringify!($act),
      //   file!(),
      //   line!()
      // );
    } else {
      $ctx.pass_count += 1;
    }
  };
}

#[macro_export]
macro_rules! check_optional {
  ($ctx:ident, $act:expr, $exp:expr) => {
    if let Some(v) = $act
      && v == $exp
    {
      $ctx.pass_count += 1;
    } else {
      $ctx.fail_count += 1;
    }
  };
}

#[macro_export]
macro_rules! mark_fail {
  ($ctx:ident, $msg:literal) => {
    $ctx.fail_count += 1;
    // debug_print!("    FAIL: {} ({} {})\n", $msg, file!(), line!());
  };
}
