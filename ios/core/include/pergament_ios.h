#ifndef PERGAMENT_IOS_H
#define PERGAMENT_IOS_H

#include <stddef.h>
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
 * exactly once for `id`. The commands the desktop app runs synchronously
 * (user data edits and reads, page rendering, the library list) run one at a
 * time in call order; the others each run on their own thread. reply normally
 * arrives on the worker thread, but may be called on the calling thread before
 * this returns if the worker cannot be reached: register `id` before calling
 * and do not hold a lock the callback takes. NULL or empty args_json means
 * JSON null. A NULL handle is ignored.
 */
void pergament_invoke(void *handle, uint64_t id, const char *name, const char *args_json);

/* Writes a rolling backup synchronously on the calling thread, on every call
   (scenePhase == .background); skipped when nothing changed or rolling is off. NULL is ignored. */
void pergament_backup(void *handle);

/* Runs the backup, then releases the handle; does not wait for calls in flight. NULL is ignored. */
void pergament_close(void *handle);

/* Frees a string returned through error_out. NULL is ignored. */
void pergament_string_free(const char *s);

/*
 * Media responses for the two schemes the web view loads media from:
 * pergament_media_scheme() (pictures) and "asset" (files in the media folder,
 * the path being "/" + encodeURIComponent(absolute path)).
 */

/* The picture scheme ("jwmedia"). Static; do not free. */
const char *pergament_media_scheme(void);

/*
 * Opens the response for `path` (the URL path, still percent-encoded) on
 * `scheme`. `range` is the Range header value or NULL. BLOCKS: an uncached
 * catalog picture is fetched from the network first, so never call this on the
 * main thread. Returns NULL for an unknown scheme, a NULL handle, scheme or
 * path, or an internal error; otherwise a response (possibly 403, 404 or 416)
 * released by pergament_media_free. The response does not reference the handle,
 * so it stays usable after pergament_close.
 */
void *pergament_media_open(void *handle, const char *scheme, const char *path, const char *range);

/* The HTTP status; 0 for NULL. */
int pergament_media_status(const void *media);

/* The headers as a JSON object of strings. Valid until pergament_media_free; do not free. */
const char *pergament_media_headers(const void *media);

/*
 * Reads up to `cap` body bytes into `buf`: returns the count, 0 at the end of
 * the body, -1 on a read error or a NULL argument. Use one response from one
 * thread at a time.
 */
intptr_t pergament_media_read(void *media, uint8_t *buf, size_t cap);

/* Releases a response. NULL is ignored. */
void pergament_media_free(void *media);

#ifdef __cplusplus
}
#endif

#endif /* PERGAMENT_IOS_H */
