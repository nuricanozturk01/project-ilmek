import { Component, OnDestroy, OnInit, signal } from '@angular/core';

interface Task {
  id: string;
  title: string;
  done: boolean;
}

@Component({
  selector: 'app-root',
  templateUrl: './app.html',
})
export class App implements OnInit, OnDestroy {
  protected readonly tasks = signal<Task[]>([]);
  protected readonly error = signal('');
  private unsubscribe?: () => void;

  async ngOnInit(): Promise<void> {
    await this.run(async () => {
      await this.load();
      this.unsubscribe = await window.ilmek.subscribe('tasks.events', null, () => {
        void this.run(() => this.load());
      });
    });
  }

  ngOnDestroy(): void {
    this.unsubscribe?.();
  }

  protected async add(input: HTMLInputElement): Promise<void> {
    const title = input.value.trim();
    if (!title) {
      return;
    }
    await this.run(async () => {
      await window.ilmek.call('tasks.create', { title }, { write: true });
      input.value = '';
      await this.load();
    });
  }

  protected async complete(task: Task): Promise<void> {
    await this.run(async () => {
      await window.ilmek.call('tasks.complete', { id: task.id }, { write: true });
      await this.load();
    });
  }

  private async load(): Promise<void> {
    const result = await window.ilmek.call<{ tasks: Task[] }>('tasks.list', {});
    this.tasks.set(result.tasks);
  }

  private async run(action: () => Promise<void>): Promise<void> {
    this.error.set('');
    try {
      await action();
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : String(err));
    }
  }
}
