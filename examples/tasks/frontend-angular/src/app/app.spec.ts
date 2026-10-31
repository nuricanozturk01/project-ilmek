import { ComponentFixture, TestBed } from '@angular/core/testing';
import type { IlmekAppSDK } from '../types/ilmek-sdk';
import { App } from './app';

interface Task {
  id: number;
  title: string;
  done: boolean;
}

interface MockInput {
  id?: number;
  title?: string;
  done?: boolean;
}

function installMock(initial: Task[]) {
  let tasks = initial.map((task) => ({ ...task }));
  let onEvent: ((event: unknown) => void) | undefined;

  const call = vi.fn(async (method: string, input: MockInput) => {
    if (method === 'tasks.list') {
      return { tasks: tasks.map((task) => ({ ...task })) };
    }
    if (method === 'tasks.create') {
      const id = Math.max(0, ...tasks.map((task) => task.id)) + 1;
      tasks.push({ id, title: input.title ?? '', done: false });
      return { task: { id, title: input.title, done: false } };
    }
    if (method === 'tasks.complete') {
      tasks = tasks.map((task) => (task.id === input.id ? { ...task, done: input.done ?? true } : task));
      return {};
    }
    if (method === 'tasks.update') {
      tasks = tasks.map((task) => (task.id === input.id ? { ...task, title: input.title ?? task.title } : task));
      return {};
    }
    if (method === 'tasks.delete') {
      tasks = tasks.filter((task) => task.id !== input.id);
      return { deleted: input.id };
    }
    if (method === 'tasks.clear_completed') {
      const before = tasks.length;
      tasks = tasks.filter((task) => !task.done);
      return { removed: before - tasks.length };
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

async function settle(fixture: ComponentFixture<App>): Promise<void> {
  await flush();
  fixture.detectChanges();
}

function itemTexts(fixture: ComponentFixture<App>): string[] {
  const titles = fixture.nativeElement.querySelectorAll('li .title') as NodeListOf<HTMLElement>;
  return Array.from(titles, (title) => title.textContent?.trim() ?? '');
}

function rows(fixture: ComponentFixture<App>): HTMLLIElement[] {
  return Array.from(fixture.nativeElement.querySelectorAll('li') as NodeListOf<HTMLLIElement>);
}

function buttonNamed(root: ParentNode, label: string): HTMLButtonElement {
  const found = Array.from(root.querySelectorAll('button')).find((button) => button.textContent?.trim() === label);
  if (!found) {
    throw new Error(`no button named "${label}"`);
  }
  return found;
}

function addInput(fixture: ComponentFixture<App>): HTMLInputElement {
  return fixture.nativeElement.querySelector('form input[type="text"]') as HTMLInputElement;
}

function errorText(fixture: ComponentFixture<App>): string {
  return (fixture.nativeElement.querySelector('.error') as HTMLElement | null)?.textContent ?? '';
}

function pressKey(element: HTMLElement, key: string): void {
  element.dispatchEvent(new KeyboardEvent('keydown', { key }));
}

describe('App (todo)', () => {
  afterEach(() => {
    delete (window as Partial<Window>).ilmek;
  });

  it('renders the list from tasks.list and strikes through completed items', async () => {
    const mock = installMock([
      { id: 1, title: 'Buy milk', done: false },
      { id: 2, title: 'Write report', done: true },
    ]);

    const fixture = await render();

    expect(mock.call).toHaveBeenCalledWith('tasks.list', {});
    expect(itemTexts(fixture)).toEqual(['Buy milk', 'Write report']);
    expect(rows(fixture)[0].classList.contains('done')).toBe(false);
    expect(rows(fixture)[1].classList.contains('done')).toBe(true);
    expect((rows(fixture)[1].querySelector('input[type="checkbox"]') as HTMLInputElement).checked).toBe(true);
  });

  it('shows the remaining count', async () => {
    installMock([
      { id: 1, title: 'Buy milk', done: false },
      { id: 2, title: 'Write report', done: true },
    ]);

    const fixture = await render();

    expect(fixture.nativeElement.textContent).toContain('1 left');
  });

  it('shows 0 left when every task is done', async () => {
    installMock([{ id: 1, title: 'Buy milk', done: true }]);

    const fixture = await render();

    expect(fixture.nativeElement.textContent).toContain('0 left');
  });

  it('shows a short empty state when there are no tasks', async () => {
    installMock([]);

    const fixture = await render();

    expect(fixture.nativeElement.textContent).toContain('Nothing to do yet.');
    expect(rows(fixture).length).toBe(0);
  });

  it('filters the list with All, Active and Completed', async () => {
    installMock([
      { id: 1, title: 'Buy milk', done: false },
      { id: 2, title: 'Write report', done: true },
    ]);
    const fixture = await render();
    const allTab = buttonNamed(fixture.nativeElement, 'All');
    const activeTab = buttonNamed(fixture.nativeElement, 'Active');
    const completedTab = buttonNamed(fixture.nativeElement, 'Completed');

    expect(allTab.getAttribute('aria-pressed')).toBe('true');
    expect(activeTab.getAttribute('aria-pressed')).toBe('false');

    activeTab.click();
    await settle(fixture);
    expect(itemTexts(fixture)).toEqual(['Buy milk']);
    expect(activeTab.getAttribute('aria-pressed')).toBe('true');
    expect(allTab.getAttribute('aria-pressed')).toBe('false');

    completedTab.click();
    await settle(fixture);
    expect(itemTexts(fixture)).toEqual(['Write report']);

    allTab.click();
    await settle(fixture);
    expect(itemTexts(fixture)).toEqual(['Buy milk', 'Write report']);
  });

  it('shows a short message when the filter has no matches', async () => {
    installMock([{ id: 1, title: 'Buy milk', done: false }]);
    const fixture = await render();

    buttonNamed(fixture.nativeElement, 'Completed').click();
    await settle(fixture);

    expect(fixture.nativeElement.textContent).toContain('Nothing here.');
    expect(rows(fixture).length).toBe(0);
  });

  it('subscribes to tasks.events on init', async () => {
    const mock = installMock([]);

    await render();

    expect(mock.subscribe).toHaveBeenCalledWith('tasks.events', null, expect.any(Function));
  });

  it('creates a task as a write call and shows it in the list', async () => {
    const mock = installMock([]);
    const fixture = await render();

    const input = addInput(fixture);
    input.value = 'Pay rent';
    input.dispatchEvent(new Event('input'));
    buttonNamed(fixture.nativeElement, 'Add').click();
    await settle(fixture);

    expect(mock.call).toHaveBeenCalledWith('tasks.create', { title: 'Pay rent' }, { write: true });
    expect(itemTexts(fixture)).toEqual(['Pay rent']);
    expect(input.value).toBe('');
  });

  it('adds a task on Enter and trims the title', async () => {
    const mock = installMock([]);
    const fixture = await render();

    const input = addInput(fixture);
    input.value = '  Pay rent  ';
    input.form?.requestSubmit();
    await settle(fixture);

    expect(mock.call).toHaveBeenCalledWith('tasks.create', { title: 'Pay rent' }, { write: true });
  });

  it('does not send an empty or whitespace-only title', async () => {
    const mock = installMock([]);
    const fixture = await render();
    const input = addInput(fixture);

    input.value = '   ';
    buttonNamed(fixture.nativeElement, 'Add').click();
    await settle(fixture);

    expect(mock.call.mock.calls.some(([method]) => method === 'tasks.create')).toBe(false);
  });

  it('limits the new task input to 200 characters', async () => {
    installMock([]);
    const fixture = await render();

    expect(addInput(fixture).maxLength).toBe(200);
  });

  it('completes a task as a write call and marks it done', async () => {
    const mock = installMock([{ id: 1, title: 'Buy milk', done: false }]);
    const fixture = await render();

    const checkbox = fixture.nativeElement.querySelector('input[type="checkbox"]') as HTMLInputElement;
    checkbox.click();
    await settle(fixture);

    expect(mock.call).toHaveBeenCalledWith('tasks.complete', { id: 1, done: true }, { write: true });
    expect(rows(fixture)[0].classList.contains('done')).toBe(true);
  });

  it('reopens a completed task with done false', async () => {
    const mock = installMock([{ id: 1, title: 'Buy milk', done: true }]);
    const fixture = await render();

    const checkbox = fixture.nativeElement.querySelector('input[type="checkbox"]') as HTMLInputElement;
    checkbox.click();
    await settle(fixture);

    expect(mock.call).toHaveBeenCalledWith('tasks.complete', { id: 1, done: false }, { write: true });
    expect(rows(fixture)[0].classList.contains('done')).toBe(false);
  });

  it('reverts the checkbox and shows an error when completing fails', async () => {
    const mock = installMock([{ id: 1, title: 'Buy milk', done: false }]);
    const fixture = await render();
    mock.call.mockImplementation(async (method: string) => {
      if (method === 'tasks.complete') {
        throw new Error('host busy');
      }
      return { tasks: [{ id: 1, title: 'Buy milk', done: false }] };
    });

    const checkbox = fixture.nativeElement.querySelector('input[type="checkbox"]') as HTMLInputElement;
    checkbox.click();
    await settle(fixture);

    expect(checkbox.checked).toBe(false);
    expect(checkbox.disabled).toBe(false);
    expect(errorText(fixture)).toContain('host busy');
  });

  it('clears the error line after the next successful action', async () => {
    const mock = installMock([{ id: 1, title: 'Buy milk', done: false }]);
    const fixture = await render();
    const original = mock.call.getMockImplementation();
    mock.call.mockImplementationOnce(async () => {
      throw new Error('host busy');
    });

    const checkbox = fixture.nativeElement.querySelector('input[type="checkbox"]') as HTMLInputElement;
    checkbox.click();
    await settle(fixture);
    expect(errorText(fixture)).toContain('host busy');

    mock.call.mockImplementation(original!);
    addInput(fixture).value = 'Water plants';
    buttonNamed(fixture.nativeElement, 'Add').click();
    await settle(fixture);

    expect(errorText(fixture)).toBe('');
  });

  it('edits a title and saves it with tasks.update on Enter', async () => {
    const mock = installMock([{ id: 1, title: 'Buy milk', done: false }]);
    const fixture = await render();

    buttonNamed(rows(fixture)[0], 'Edit').click();
    fixture.detectChanges();
    const editor = rows(fixture)[0].querySelector('input[type="text"]') as HTMLInputElement;
    expect(editor.value).toBe('Buy milk');

    editor.value = '  Buy oat milk ';
    pressKey(editor, 'Enter');
    await settle(fixture);

    expect(mock.call).toHaveBeenCalledWith('tasks.update', { id: 1, title: 'Buy oat milk' }, { write: true });
    expect(itemTexts(fixture)).toEqual(['Buy oat milk']);
  });

  it('leaves edit mode without calling tasks.update when the title is unchanged', async () => {
    const mock = installMock([{ id: 1, title: 'Buy milk', done: false }]);
    const fixture = await render();

    buttonNamed(rows(fixture)[0], 'Edit').click();
    fixture.detectChanges();
    const editor = rows(fixture)[0].querySelector('input[type="text"]') as HTMLInputElement;
    editor.value = '  Buy milk ';
    pressKey(editor, 'Enter');
    await settle(fixture);

    expect(mock.call.mock.calls.some(([method]) => method === 'tasks.update')).toBe(false);
    expect(rows(fixture)[0].querySelector('input[type="text"]')).toBeNull();
    expect(itemTexts(fixture)).toEqual(['Buy milk']);
  });

  it('cancels an edit on Escape without calling tasks.update', async () => {
    const mock = installMock([{ id: 1, title: 'Buy milk', done: false }]);
    const fixture = await render();

    buttonNamed(rows(fixture)[0], 'Edit').click();
    fixture.detectChanges();
    const editor = rows(fixture)[0].querySelector('input[type="text"]') as HTMLInputElement;
    editor.value = 'Something else';
    pressKey(editor, 'Escape');
    await settle(fixture);

    expect(mock.call.mock.calls.some(([method]) => method === 'tasks.update')).toBe(false);
    expect(rows(fixture)[0].querySelector('input[type="text"]')).toBeNull();
    expect(itemTexts(fixture)).toEqual(['Buy milk']);
  });

  it('deletes a task with tasks.delete and reloads the list', async () => {
    const mock = installMock([
      { id: 1, title: 'Buy milk', done: false },
      { id: 2, title: 'Write report', done: false },
    ]);
    const fixture = await render();
    const listCalls = () => mock.call.mock.calls.filter(([method]) => method === 'tasks.list').length;
    const listsBefore = listCalls();

    buttonNamed(rows(fixture)[0], 'Delete').click();
    await settle(fixture);

    expect(mock.call).toHaveBeenCalledWith('tasks.delete', { id: 1 }, { write: true });
    expect(listCalls()).toBeGreaterThan(listsBefore);
    expect(itemTexts(fixture)).toEqual(['Write report']);
  });

  it('shows Clear completed only when a completed task exists', async () => {
    installMock([{ id: 1, title: 'Buy milk', done: false }]);
    const fixture = await render();

    expect(fixture.nativeElement.textContent).not.toContain('Clear completed');
    expect(Array.from(fixture.nativeElement.querySelectorAll('button')).some((button) => (button as HTMLButtonElement).textContent?.trim() === 'Clear completed')).toBe(false);
  });

  it('clears completed tasks with tasks.clear_completed', async () => {
    const mock = installMock([
      { id: 1, title: 'Buy milk', done: true },
      { id: 2, title: 'Write report', done: false },
    ]);
    const fixture = await render();

    buttonNamed(fixture.nativeElement, 'Clear completed').click();
    await settle(fixture);

    expect(mock.call).toHaveBeenCalledWith('tasks.clear_completed', {}, { write: true });
    expect(itemTexts(fixture)).toEqual(['Write report']);
    expect(Array.from(fixture.nativeElement.querySelectorAll('button')).some((button) => (button as HTMLButtonElement).textContent?.trim() === 'Clear completed')).toBe(false);
  });

  it('refreshes the list when a tasks.events event arrives', async () => {
    const mock = installMock([{ id: 1, title: 'Buy milk', done: false }]);
    const fixture = await render();

    mock.addRemoteTask({ id: 2, title: 'Water plants', done: false });
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

    expect(errorText(fixture)).toContain('host unavailable');
  });
});
