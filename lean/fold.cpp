#include <exception>
#include <gmp.h>
#include <lean/lean.h>
using std::exception, std::exception_ptr;

extern "C" {
lean_object *
l_Lean_Language_SnapshotTree_foldM___at___00main_spec__8(lean_object *,
                                                         lean_object *);
size_t strlen(const char *);
ssize_t write(int, const void *, size_t);
}

void cc() {
  const char *s = 0;
  int len = 0;
  try {
    exception_ptr eptr{std::current_exception()};
    if (eptr) {
      std::rethrow_exception(eptr);
    }
  } catch (exception &e) {
    s = e.what();
    len = strlen(s);
  } catch (...) {
  }
  write(1, "\x08\x01", 2);
  write(1, &len, 4);
  write(1, s, len);
  write(1, "\x00", 1);
  std::exit(0);
}

extern "C" lean_object *protect(lean_object *arg1, lean_object *arg2) {
  std::set_terminate(cc);
  try {
    return l_Lean_Language_SnapshotTree_foldM___at___00main_spec__8(arg1, arg2);
  } catch (exception &e) {
    lean_object *s = lean_mk_string(e.what());
    lean_object *err = lean_mk_io_user_error(s);
    return lean_io_result_mk_error(err);
  }
}

#include <stdio.h>
extern "C" uint8_t isMalform_literal(lean_object *lit) {
  switch (lit->m_tag) {
  case 0: {
    lean_object *nat = lean_ctor_get(lit, 0);
    if (!lean_is_scalar(nat) && lean_is_mpz(nat)) {
      mpz_ptr r = (mpz_ptr)(nat + 1);
      return r->_mp_size < 0;
    }
    break;
  }
  }
  return 0;
}
