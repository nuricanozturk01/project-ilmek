import { Component, OnDestroy, OnInit, computed, signal } from '@angular/core';

interface Task {
  id: number;
  title: string;
  done: boolean;
}

type Filter = 'all' | 'active' | 'completed';

const WRITE = { write: true } as const;

@Component({
  selector: 'app-root',
  templateUrl: './app.html',
})
export class App implements OnInit, OnDestroy {
  protected readonly tasks = signal<Task[]>([]);
  protected readonly error = signal('');
  protected readonly filter = signal<Filter>('all');
  protected readonly editingId = signal<number | null>(null);
  protected readonly remaining = computed(() => this.tasks().filter((task) => !task.done).length);
  protected readonly completedCount = computed(() => this.tasks().length - this.remaining());
  protected readonly visible = computed(() => {
    const filter = this.filter();
    return this.tasks().filter((task) =>
      filter === 'all' || (filter === 'active' ? !task.done : task.done),
    );
  });
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

  protected setFilter(filter: Filter): void {
    this.filter.set(filter);
  }

  protected async add(input: HTMLInputElement): Promise<void> {
    const title = input.value.trim();
    if (!title) {
      return;
    }
    await this.run(async () => {
      await window.ilmek.call('tasks.create', { title }, WRITE);
      input.value = '';
      await this.load();
    });
  }

  protected async complete(task: Task, checkbox: HTMLInputElement): Promise<void> {
    await this.run(async () => {
      try {
        await window.ilmek.call('tasks.complete', { id: task.id, done: checkbox.checked }, WRITE);
      } catch (err) {
        checkbox.checked = task.done;
        await this.load().catch(() => undefined);
        throw err;
      }
      await this.load();
    });
  }

  protected startEdit(task: Task): void {
    this.editingId.set(task.id);
  }

  protected cancelEdit(): void {
    this.editingId.set(null);
  }

  protected async saveEdit(task: Task, input: HTMLInputElement): Promise<void> {
    const title = input.value.trim();
    if (!title || title === task.title) {
      this.cancelEdit();
      return;
    }
    await this.run(async () => {
      await window.ilmek.call('tasks.update', { id: task.id, title }, WRITE);
      this.editingId.set(null);
      await this.load();
    });
  }

  protected async remove(task: Task): Promise<void> {
    await this.run(async () => {
      await window.ilmek.call('tasks.delete', { id: task.id }, WRITE);
      await this.load();
    });
  }

  protected async clearCompleted(): Promise<void> {
    await this.run(async () => {
      await window.ilmek.call('tasks.clear_completed', {}, WRITE);
      await this.load();
    });
  }

  private async load(): Promise<void> {
    const result = await window.ilmek.call<{ tasks: Task[] }>('tasks.list', {});
    this.tasks.set(result.tasks);
  }

  private async run(action: () => Promise<void>): Promise<void> {
    try {
      await action();
      this.error.set('');
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : String(err));
    }
  }
}
