/*
 * Derived from SOEM `osal/linux/osal.c`.
 *
 * SOEM is dual-licensed under GPLv3 and a commercial license; this crate uses
 * it under GPLv3 (the GPL-3.0 text ships with this crate as COPYING). The
 * canonical SOEM source and its LICENSE.md live upstream:
 * https://github.com/OpenEtherCATsociety/SOEM
 *
 * Modification for autd3-rs-link-soem (2026-06): `clock_nanosleep` (not
 * available on macOS) replaced with `nanosleep` in `osal_usleep` and
 * `osal_monotonic_sleep`.
 */
#include <osal.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <sched.h>

/* Returns time from some unspecified moment in past,
 * strictly increasing, used for time intervals measurement. */
void osal_get_monotonic_time(ec_timet *ts)
{
   clock_gettime(CLOCK_MONOTONIC, ts);
}

ec_timet osal_current_time(void)
{
   struct timespec ts;

   clock_gettime(CLOCK_REALTIME, &ts);
   return ts;
}

void osal_time_diff(ec_timet *start, ec_timet *end, ec_timet *diff)
{
   osal_timespecsub(end, start, diff);
}

void osal_timer_start(osal_timert *self, uint32 timeout_usec)
{
   struct timespec start_time;
   struct timespec timeout;

   osal_get_monotonic_time(&start_time);
   osal_timespec_from_usec(timeout_usec, &timeout);
   osal_timespecadd(&start_time, &timeout, &self->stop_time);
}

boolean osal_timer_is_expired(osal_timert *self)
{
   struct timespec current_time;
   int is_not_yet_expired;

   osal_get_monotonic_time(&current_time);
   is_not_yet_expired = osal_timespeccmp(&current_time, &self->stop_time, <);

   return is_not_yet_expired == FALSE;
}

/* macOS has no clock_nanosleep, so sleep relative to the current monotonic
 * clock using nanosleep instead. */
int osal_usleep(uint32 usec)
{
   struct timespec ts;
   int result;

   osal_timespec_from_usec(usec, &ts);
   result = nanosleep(&ts, NULL);
   return result == 0 ? 0 : -1;
}

int osal_monotonic_sleep(ec_timet *ts)
{
   struct timespec now;
   struct timespec delay;
   int result;

   osal_get_monotonic_time(&now);
   if (!osal_timespeccmp(&now, ts, <))
   {
      return 0;
   }
   osal_timespecsub(ts, &now, &delay);
   result = nanosleep(&delay, NULL);
   return result == 0 ? 0 : -1;
}

void *osal_malloc(size_t size)
{
   return malloc(size);
}

void osal_free(void *ptr)
{
   free(ptr);
}

int osal_thread_create(void *thandle, int stacksize, void *func, void *param)
{
   int ret;
   pthread_attr_t attr;
   pthread_t *threadp;

   threadp = thandle;
   pthread_attr_init(&attr);
   pthread_attr_setstacksize(&attr, stacksize);
   ret = pthread_create(threadp, &attr, func, param);
   if (ret < 0)
   {
      return 0;
   }
   return 1;
}

int osal_thread_create_rt(void *thandle, int stacksize, void *func, void *param)
{
   int ret;
   pthread_attr_t attr;
   struct sched_param schparam;
   pthread_t *threadp;

   threadp = thandle;
   pthread_attr_init(&attr);
   pthread_attr_setstacksize(&attr, stacksize);
   ret = pthread_create(threadp, &attr, func, param);
   pthread_attr_destroy(&attr);
   if (ret < 0)
   {
      return 0;
   }
   memset(&schparam, 0, sizeof(schparam));
   schparam.sched_priority = 40;
   ret = pthread_setschedparam(*threadp, SCHED_FIFO, &schparam);
   if (ret < 0)
   {
      return 0;
   }

   return 1;
}

void *osal_mutex_create(void)
{
   pthread_mutexattr_t mutexattr;
   osal_mutext *mutex;
   mutex = (osal_mutext *)osal_malloc(sizeof(osal_mutext));
   if (mutex)
   {
      pthread_mutexattr_init(&mutexattr);
      pthread_mutexattr_setprotocol(&mutexattr, PTHREAD_PRIO_INHERIT);
      pthread_mutex_init(mutex, &mutexattr);
   }
   return (void *)mutex;
}

void osal_mutex_destroy(void *mutex)
{
   pthread_mutex_destroy((osal_mutext *)mutex);
   osal_free(mutex);
}

void osal_mutex_lock(void *mutex)
{
   pthread_mutex_lock((osal_mutext *)mutex);
}

void osal_mutex_unlock(void *mutex)
{
   pthread_mutex_unlock((osal_mutext *)mutex);
}
