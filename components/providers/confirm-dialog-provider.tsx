'use client';

import * as React from 'react';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { buttonVariants } from '@/components/ui/button';
import { cn } from '@/lib/utils';

export interface ConfirmDialogOptions {
  title: string;
  description?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  /** Destructive styling for irreversible or high-risk actions */
  confirmVariant?: 'default' | 'destructive';
}

type ConfirmDialogContextValue = {
  confirmDialog: (options: ConfirmDialogOptions) => Promise<boolean>;
};

const ConfirmDialogContext = React.createContext<ConfirmDialogContextValue | null>(null);

export function ConfirmDialogProvider({ children }: { children: React.ReactNode }) {
  const [open, setOpen] = React.useState(false);
  const [title, setTitle] = React.useState('');
  const [description, setDescription] = React.useState('');
  const [confirmLabel, setConfirmLabel] = React.useState('Confirmar');
  const [cancelLabel, setCancelLabel] = React.useState('Cancelar');
  const [confirmVariant, setConfirmVariant] = React.useState<'default' | 'destructive'>('default');
  const resolveRef = React.useRef<((value: boolean) => void) | null>(null);

  const confirmDialog = React.useCallback((options: ConfirmDialogOptions) => {
    setTitle(options.title);
    setDescription(options.description ?? '');
    setConfirmLabel(options.confirmLabel ?? 'Confirmar');
    setCancelLabel(options.cancelLabel ?? 'Cancelar');
    setConfirmVariant(options.confirmVariant ?? 'default');
    setOpen(true);
    return new Promise<boolean>((resolve) => {
      resolveRef.current = resolve;
    });
  }, []);

  const handleOpenChange = React.useCallback((open: boolean) => {
    if (!open && resolveRef.current) {
      resolveRef.current(false);
      resolveRef.current = null;
    }
    setOpen(open);
  }, []);

  const handleConfirm = React.useCallback(() => {
    resolveRef.current?.(true);
    resolveRef.current = null;
    setOpen(false);
  }, []);

  const handleCancel = React.useCallback(() => {
    resolveRef.current?.(false);
    resolveRef.current = null;
    setOpen(false);
  }, []);

  const value = React.useMemo(() => ({ confirmDialog }), [confirmDialog]);

  return (
    <ConfirmDialogContext.Provider value={value}>
      {children}
      <AlertDialog open={open} onOpenChange={handleOpenChange}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{title}</AlertDialogTitle>
            {description ? (
              <AlertDialogDescription>{description}</AlertDialogDescription>
            ) : null}
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel onClick={handleCancel}>{cancelLabel}</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleConfirm}
              className={cn(
                confirmVariant === 'destructive' ? buttonVariants({ variant: 'destructive' }) : undefined,
              )}
            >
              {confirmLabel}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </ConfirmDialogContext.Provider>
  );
}

export function useConfirmDialog(): ConfirmDialogContextValue {
  const ctx = React.useContext(ConfirmDialogContext);
  if (!ctx) {
    throw new Error('useConfirmDialog must be used within ConfirmDialogProvider');
  }
  return ctx;
}
