import { useDemoTasks } from '../context/useDemoTasks.js';

export default function TasksPage({ onSelectTask, onNewTask }) {
  const { tasks: taskList, storageError } = useDemoTasks();

  return (
    <div
      style={{
        maxWidth: '1240px',
        margin: '0 auto',
        padding: '2rem 3.5rem 120px',
      }}
    >
      {/* Header matching Page 3: TASKS + NEW TASK */}
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: '2.5rem',
          flexWrap: 'wrap',
          gap: '1rem',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '1.25rem' }}>
          <img
            src="/logo.png"
            alt="Kamish Dragon Logo"
            style={{
              width: '56px',
              height: '56px',
              objectFit: 'contain',
              filter: 'drop-shadow(0 0 12px rgba(192, 132, 252, 0.7))',
            }}
          />
          <h1
            style={{
              fontFamily: 'var(--font-main)',
              fontSize: '3.5rem',
              fontWeight: 900,
              letterSpacing: '0.04em',
              color: '#ffffff',
              margin: 0,
              textTransform: 'uppercase',
            }}
          >
            TASKS
          </h1>
        </div>

        <button
          onClick={onNewTask}
          className="brutal-btn-primary"
          style={{
            padding: '0.75rem 1.75rem',
            fontSize: '0.95rem',
            fontWeight: 800,
          }}
        >
          + NEW TASK
        </button>
      </div>

      <p>Local demo tasks and prototype status — not live verification results.</p>
      {storageError && <p role="alert">{storageError}</p>}
      {/* Task Rows matching Page 3 */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: '1.25rem' }}>
        {taskList.map((task) => (
          <div
            key={task.id}
            role="button"
            tabIndex={0}
            onKeyDown={event => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                onSelectTask(task.id);
              }
            }}
            onClick={() => onSelectTask(task.id)}
            style={{
              background: '#120b22',
              border: '2px solid #ffffff',
              boxShadow: '6px 6px 0px var(--purple-glow)',
              padding: '1.25rem 2rem',
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              cursor: 'pointer',
              flexWrap: 'wrap',
              gap: '1rem',
              transition: 'all 0.15s ease',
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.transform = 'translate(-2px, -2px)';
              e.currentTarget.style.boxShadow = '8px 8px 0px var(--purple-accent)';
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.transform = 'translate(0px, 0px)';
              e.currentTarget.style.boxShadow = '6px 6px 0px var(--purple-glow)';
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '2rem', minWidth: 0, flex: 1 }}>
              <span
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '1rem',
                  fontWeight: 700,
                  color: '#ffffff',
                  minWidth: '90px',
                }}
              >
                {task.id.startsWith('DEMO-') ? `${task.id.slice(0, 13)}…` : task.id}
              </span>
              <span
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.95rem',
                  color: '#e2d9f3',
                  overflowWrap: 'anywhere',
                }}
              >
                [{task.title}]
              </span>
            </div>

            <div style={{ display: 'flex', alignItems: 'center', gap: '1.5rem' }}>
              <span
                style={{
                  fontFamily: 'var(--font-mono)',
                  fontSize: '0.9rem',
                  color: '#ffffff',
                  fontWeight: 700,
                  border: '1px solid #ffffff',
                  padding: '3px 10px',
                  borderRadius: '2px',
                }}
              >
                [{task.status}]
              </span>
              <span style={{ fontSize: '1.2rem', color: '#c084fc' }}>→</span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
