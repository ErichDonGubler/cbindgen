#ifdef CBINDGEN_CYTHON
from libc.stdint cimport uint64_t
ctypedef uint64_t Option_Foo
#else
#include <stdint.h>
typedef uint64_t Option_Foo;
#endif


#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct {
  Option_Foo foo;
} Bar;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

void root(Bar f);

#ifdef __cplusplus
} // extern "C"
#endif // __cplusplus
