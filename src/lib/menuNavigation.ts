import type { KeyboardEvent } from 'react';

/** Common keyboard navigation for in-app menus, independent of their anchor. */
export function focusMenuItem(menu: HTMLElement | null, last = false) {
  const items = enabledItems(menu);
  (last ? items[items.length - 1] : items[0])?.focus();
}

function enabledItems(menu: HTMLElement | null) {
  return Array.from(menu?.querySelectorAll<HTMLElement>(
    '[role="menuitem"]:not(:disabled):not([aria-disabled="true"])'
  ) ?? []);
}

export function handleMenuNavigation(event: KeyboardEvent<HTMLElement>) {
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
  const items = enabledItems(event.currentTarget);
  if (!items.length) return;
  const current = items.indexOf(document.activeElement as HTMLElement);
  const next = event.key === 'Home' ? 0
    : event.key === 'End' ? items.length - 1
    : event.key === 'ArrowDown' ? (current + 1) % items.length
    : (current < 0 ? items.length - 1 : (current - 1 + items.length) % items.length);
  event.preventDefault();
  event.stopPropagation();
  items[next].focus();
}
