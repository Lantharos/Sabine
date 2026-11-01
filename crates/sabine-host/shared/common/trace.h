#ifndef SABINE_CEF_HOST_COMMON_TRACE_H_
#define SABINE_CEF_HOST_COMMON_TRACE_H_

// Whether SABINE_TRACE asks for host tracing on stderr, read as the window
// host reads it: 1, true, yes, on, or trace in any case.
bool TraceEnabled();

#endif
