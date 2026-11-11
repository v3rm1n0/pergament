#ifndef PERGAMENT_IOS_H
#define PERGAMENT_IOS_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Threading: both callbacks arrive on arbitrary threads. `ctx` and both
 * callbacks must stay valid until every issued id has replied.
 * Strings passed to callbacks are valid only for the duration of the call;
 * copy them.
 *
 * reply: ok = 1 carries the result as JSON; ok = 0 carries the error as a
 * JSON string (quoted and escaped).
 * event: `event` is the event name, `json` the payload serialized alone.
 */
typedef void (*pergament_reply_fn)(void *ctx, uint64_t id, int ok, const char *json);
typedef void (*pergament_event_fn)(void *ctx, const char *event, const char *json);

/* The core version as a NUL-terminated UTF-8 string. Static; do not free. */
const char *pergament_ios_version(void);

/*
 * Open the core. Returns NULL on failure; then, if error_out is non-NULL,
 * *error_out is a message the caller frees with pergament_string_free.
 * On success *error_out is left NULL. The handle is released by pergament_close.
 */
void *pergament_open(const char *library_dir, const char *cache_dir,
                     void *ctx, pergament_reply_fn reply, pergament_event_fn event,
                     const char **error_out);

/*
 * Returns immediately; runs the command on a worker thread and calls reply
 * exactly once for `id`. NULL or empty args_json means JSON null. A NULL
 * handle is ignored.
 */
void pergament_invoke(void *handle, uint64_t id, const char *name, const char *args_json);

/* Runs the close-time backup synchronously on the calling thread (scenePhase == .background). */
void pergament_backup(void *handle);

/* Runs the backup, then releases the handle; does not wait for calls in flight. NULL is ignored. */
void pergament_close(void *handle);

/* Frees a string returned through error_out. NULL is ignored. */
void pergament_string_free(char *s);

#ifdef __cplusplus
}
#endif

#endif /* PERGAMENT_IOS_H */
