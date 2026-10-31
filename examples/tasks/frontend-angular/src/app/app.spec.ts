import { ComponentFixture, TestBed } from '@angular/core/testing';
import type { IlmekAppSDK } from '../types/ilmek-sdk';
import { App } from './app';

interface Task {
  id: string;
  title: string;
  done: boolean;
}

function installMock(initial: Task[]) {
  let tasks = initial.map((task) => ({ ...task }));
  let onEvent: ((event: unknown) => void) | undefined;

  const call = vi.fn(async (method: string, input: { title?: string; id?: string }) => {
    if (method === 'tasks.list') {
      return { tasks: tasks.map((task) => ({ ...task })) };
    }
    if (method === 'tasks.create') {
      tasks.push({ id: `task-${tasks.length + 1}`, title: input.title ?? '', done: false });
      return {};
    }
    if (method === 'tasks.complete') {
      tasks = tasks.map((task) => (task.id === input.id ? { ...task, done: true } : task));
      return {};
    }
    throw new Error(`unexpected method ${method}`);
  });

  const subscribe = vi.fn(async (_topic: string, _cursor: string | null, handler: (event: unknown) => void) => {
    onEvent = handler;
    return () => {};
  });

  window.ilmek = { call, subscribe } as unknown as IlmekAppSDK;

  return {
    call,
    subscribe,
    addRemoteTask(task: Task) {
      tasks.push(task);
      onEvent?.({ type: 'tasks.changed' });
    },
  };
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

async function render(): Promise<ComponentFixture<App>> {
  const fixture = TestBed.createComponent(App);
  fixture.detectChanges();
  await flush();
  fixture.detectChanges();
  return fixture;
}

function itemTexts(fixture: ComponentFixture<App>): string[] {
  const items = fixture.nativeElement.querySelectorAll('li') as NodeListOf<HTMLLIElement>;
  return Array.from(items, (item) => item.textContent?.trim() ?? '');
}

describe('App (tasks)', () => {
  afterEach(() => {
    delete (window as Partial<Window>).ilmek;
  });

  it('renders the list from tasks.list and strikes through completed items', async () => {
    const mock = installMock([
      { id: 'task-1', title: 'Buy milk', done: false },
      { id: 'task-2', title: 'Write report', done: true },
    ]);

    const fixture = await render();

    expect(mock.call).toHaveBeenCalledWith('tasks.list', {});
    expect(itemTexts(fixture)).toEqual(['Buy milk', 'Write report']);

    const items = fixture.nativeElement.querySelectorAll('li') as NodeListOf<HTMLLIElement>;
    expect(items[0].classList.contains('done')).toBe(false);
    expect(items[1].classList.contains('done')).toBe(true);
    expect((items[1].querySelector('input[type="checkbox"]') as HTMLInputElement).checked).toBe(true);
  });

  it('shows an empty-state message when there are no tasks', async () => {
    installMock([]);

    const fixture = await render();

    expect(fixture.nativeElement.textContent).toContain('No tasks yet.');
    expect(fixture.nativeElement.querySelectorAll('li').length).toBe(0);
  });

  it('subscribes to tasks.events on init', async () => {
    const mock = installMock([]);

    await render();

    expect(mock.subscribe).toHaveBeenCalledWith('tasks.events', null, expect.any(Function));
  });

  it('creates a task as a write call and shows it in the list', async () => {
    const mock = installMock([]);
    const fixture = await render();

    const input = fixture.nativeElement.querySelector('input[type="text"]') as HTMLInputElement;
    input.value = 'Pay rent';
    input.dispatchEvent(new Event('input'));
    (fixture.nativeElement.querySelector('button') as HTMLButtonElement).click();
    await flush();
    fixture.detectChanges();

    expect(mock.call).toHaveBeenCalledWith('tasks.create', { title: 'Pay rent' }, { write: true });
    expect(itemTexts(fixture)).toEqual(['Pay rent']);
    expect(input.value).toBe('');
  });

  it('completes a task as a write call and marks it done', async () => {
    const mock = installMock([{ id: 'task-1', title: 'Buy milk', done: false }]);
    const fixture = await render();

    const checkbox = fixture.nativeElement.querySelector('input[type="checkbox"]') as HTMLInputElement;
    checkbox.click();
    await flush();
    fixture.detectChanges();

    expect(mock.call).toHaveBeenCalledWith('tasks.complete', { id: 'task-1' }, { write: true });
    const item = fixture.nativeElement.querySelector('li') as HTMLLIElement;
    expect(item.classList.contains('done')).toBe(true);
  });

  it('refreshes the list when a tasks.events event arrives', async () => {
    const mock = installMock([{ id: 'task-1', title: 'Buy milk', done: false }]);
    const fixture = await render();

    mock.addRemoteTask({ id: 'task-2', title: 'Water plants', done: false });
    await flush();
    fixture.detectChanges();

    expect(itemTexts(fixture)).toEqual(['Buy milk', 'Water plants']);
  });

  it('shows a short error line when a call fails', async () => {
    installMock([]);
    window.ilmek.call = vi.fn(async () => {
      throw new Error('host unavailable');
    }) as unknown as IlmekAppSDK['call'];

    const fixture = await render();

    const error = fixture.nativeElement.querySelector('.error') as HTMLElement;
    expect(error.textContent).toContain('host unavailable');
  });
});
