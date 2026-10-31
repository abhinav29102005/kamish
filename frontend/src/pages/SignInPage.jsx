export default function SignInPage({ onSignedIn }) {
  return (
    <section style={{ maxWidth: '680px', margin: '0 auto', padding: '2.5rem 2rem 120px', textAlign: 'center' }}>
      <img
        src="/logo.png"
        alt="Kamish Dragon Logo"
        style={{
          width: '96px',
          height: '96px',
          objectFit: 'contain',
          marginBottom: '1rem',
          filter: 'drop-shadow(0 0 16px rgba(192, 132, 252, 0.6))',
        }}
      />
      <h1>DEMO ACCESS</h1>
      <div className="brutal-card-offset" style={{ padding: '2.5rem', textAlign: 'left' }}>
        <p>This prototype has no user authentication service. Do not enter passwords or credentials.</p>
        <p>The dashboard contains simulated data and is not connected to a broker.</p>
        <button className="brutal-btn-primary" onClick={onSignedIn}>OPEN DEMO DASHBOARD</button>
      </div>
    </section>
  );
}
