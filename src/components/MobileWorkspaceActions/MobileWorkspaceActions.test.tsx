import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { MobileWorkspaceActions } from './MobileWorkspaceActions';

afterEach(cleanup);

const labels = {
  sftp: 'SFTP',
  more: 'More actions',
};

describe('MobileWorkspaceActions', () => {
  it('keeps SFTP directly accessible and moves secondary actions into a menu', () => {
    const onToggleSftp = vi.fn();
    const onSettings = vi.fn();

    render(
      <MobileWorkspaceActions
        isSftpOpen={false}
        sftpDisabled={false}
        labels={labels}
        menuItems={[
          { id: 'settings', label: 'Settings', icon: <span aria-hidden="true">S</span>, onSelect: onSettings },
        ]}
        onToggleSftp={onToggleSftp}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: 'SFTP' }));
    expect(onToggleSftp).toHaveBeenCalledOnce();

    const moreButton = screen.getByRole('button', { name: 'More actions' });
    fireEvent.click(moreButton);
    fireEvent.click(screen.getByRole('menuitem', { name: 'Settings' }));
    expect(onSettings).toHaveBeenCalledOnce();
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(moreButton).toHaveFocus();
  });
});
