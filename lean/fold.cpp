#include <exception>
#include <lean/lean.h>
using std::exception;

extern "C" {
lean_object *
l_Lean_Language_SnapshotTree_foldM___at___00main_spec__8(lean_object *,
                                                         lean_object *);
}

extern "C" lean_object *protect(lean_object *arg1, lean_object *arg2) {
  try {
    return l_Lean_Language_SnapshotTree_foldM___at___00main_spec__8(arg1, arg2);
  } catch (exception &e) {
    lean_object *s = lean_mk_string(e.what());
    lean_object *err = lean_mk_io_user_error(s);
    return lean_io_result_mk_error(err);
  }
}
