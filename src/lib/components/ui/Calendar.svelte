<script lang="ts">
  import { cn } from '$lib/utils';
  import { buttonVariants } from './button-variants';

  interface Props {
    selected?: Date;
    onSelect?: (date: Date | undefined) => void;
    disabled?: (date: Date) => boolean;
    class?: string;
    month?: Date;
    showOutsideDays?: boolean;
  }

  let {
    selected = $bindable(undefined),
    onSelect,
    disabled,
    class: className,
    month = $bindable(new Date()),
    showOutsideDays = true
  }: Props = $props();

  const DAYS = ['Su', 'Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa'];
  const MONTHS = [
    'January',
    'February',
    'March',
    'April',
    'May',
    'June',
    'July',
    'August',
    'September',
    'October',
    'November',
    'December'
  ];

  let currentMonth = $state(month.getMonth());
  let currentYear = $state(month.getFullYear());

  function getDaysInMonth(year: number, month: number) {
    return new Date(year, month + 1, 0).getDate();
  }

  function getFirstDayOfMonth(year: number, month: number) {
    return new Date(year, month, 1).getDay();
  }

  let calendarDays = $derived.by(() => {
    const daysInMonth = getDaysInMonth(currentYear, currentMonth);
    const firstDay = getFirstDayOfMonth(currentYear, currentMonth);
    const daysInPrevMonth = getDaysInMonth(currentYear, currentMonth - 1);

    const days: Array<{
      date: Date;
      isCurrentMonth: boolean;
      isToday: boolean;
      isSelected: boolean;
    }> = [];

    // Previous month's days
    for (let i = firstDay - 1; i >= 0; i--) {
      const date = new Date(currentYear, currentMonth - 1, daysInPrevMonth - i);
      days.push({
        date,
        isCurrentMonth: false,
        isToday: false,
        isSelected: false
      });
    }

    // Current month's days
    const today = new Date();
    today.setHours(0, 0, 0, 0);

    for (let i = 1; i <= daysInMonth; i++) {
      const date = new Date(currentYear, currentMonth, i);
      date.setHours(0, 0, 0, 0);

      const isToday =
        date.getDate() === today.getDate() &&
        date.getMonth() === today.getMonth() &&
        date.getFullYear() === today.getFullYear();

      const isSelected =
        selected !== undefined &&
        date.getDate() === selected.getDate() &&
        date.getMonth() === selected.getMonth() &&
        date.getFullYear() === selected.getFullYear();

      days.push({
        date,
        isCurrentMonth: true,
        isToday,
        isSelected
      });
    }

    // Next month's days
    const remainingDays = 42 - days.length; // 6 rows * 7 days
    for (let i = 1; i <= remainingDays; i++) {
      const date = new Date(currentYear, currentMonth + 1, i);
      days.push({
        date,
        isCurrentMonth: false,
        isToday: false,
        isSelected: false
      });
    }

    return days;
  });

  function previousMonth() {
    if (currentMonth === 0) {
      currentMonth = 11;
      currentYear--;
    } else {
      currentMonth--;
    }
    month = new Date(currentYear, currentMonth);
  }

  function nextMonth() {
    if (currentMonth === 11) {
      currentMonth = 0;
      currentYear++;
    } else {
      currentMonth++;
    }
    month = new Date(currentYear, currentMonth);
  }

  function selectDate(date: Date, isCurrentMonth: boolean) {
    if (!isCurrentMonth) return;
    if (disabled?.(date)) return;

    selected = date;
    onSelect?.(date);
  }

  function isDisabled(date: Date) {
    return disabled?.(date) ?? false;
  }
</script>

<div class={cn('p-3', className)}>
  <div class="space-y-4">
    <!-- Header -->
    <div class="flex justify-center pt-1 relative items-center">
      <button
        type="button"
        class={cn(
          buttonVariants({ variant: 'outline' }),
          'h-7 w-7 bg-transparent p-0 opacity-50 hover:opacity-100 absolute left-1'
        )}
        onclick={previousMonth}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          class="h-4 w-4"
        >
          <path d="m15 18-6-6 6-6" />
        </svg>
      </button>

      <div class="text-sm font-medium">
        {MONTHS[currentMonth]}
        {currentYear}
      </div>

      <button
        type="button"
        class={cn(
          buttonVariants({ variant: 'outline' }),
          'h-7 w-7 bg-transparent p-0 opacity-50 hover:opacity-100 absolute right-1'
        )}
        onclick={nextMonth}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          class="h-4 w-4"
        >
          <path d="m9 18 6-6-6-6" />
        </svg>
      </button>
    </div>

    <!-- Calendar Grid -->
    <table class="w-full border-collapse space-y-1">
      <thead>
        <tr class="flex">
          {#each DAYS as day}
            <th class="text-muted-foreground rounded-md w-9 font-normal text-[0.8rem]">
              {day}
            </th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each { length: 6 } as _, weekIndex}
          <tr class="flex w-full mt-2">
            {#each calendarDays.slice(weekIndex * 7, (weekIndex + 1) * 7) as day}
              <td class="h-9 w-9 text-center text-sm p-0 relative">
                {#if showOutsideDays || day.isCurrentMonth}
                  <button
                    type="button"
                    class={cn(
                      buttonVariants({ variant: 'ghost' }),
                      'h-9 w-9 p-0 font-normal',
                      day.isSelected &&
                        'bg-primary text-primary-foreground hover:bg-primary hover:text-primary-foreground focus:bg-primary focus:text-primary-foreground',
                      day.isToday && !day.isSelected && 'bg-accent text-accent-foreground',
                      !day.isCurrentMonth && 'text-muted-foreground opacity-50',
                      isDisabled(day.date) && 'text-muted-foreground opacity-50 cursor-not-allowed'
                    )}
                    onclick={() => selectDate(day.date, day.isCurrentMonth)}
                    disabled={isDisabled(day.date)}
                  >
                    {day.date.getDate()}
                  </button>
                {/if}
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</div>
