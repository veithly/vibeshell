import { create } from 'zustand';

/**
 * Available views in the application
 */
export type AppView = 'main' | 'settings';

/**
 * Navigation store state and actions
 */
interface NavigationStore {
  /** Currently active view */
  currentView: AppView;
  /** Navigate to a specific view */
  navigateTo: (view: AppView) => void;
  /** Go back to main view */
  goToMain: () => void;
  /** Go to settings view */
  goToSettings: () => void;
}

/**
 * Zustand store for managing application navigation
 */
export const useNavigationStore = create<NavigationStore>((set) => ({
  currentView: 'main',

  navigateTo: (view) => {
    set({ currentView: view });
  },

  goToMain: () => {
    set({ currentView: 'main' });
  },

  goToSettings: () => {
    set({ currentView: 'settings' });
  },
}));
