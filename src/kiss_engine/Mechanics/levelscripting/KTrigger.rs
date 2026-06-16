struct Trigger {
    pub m_fTriggerResetTime: f32 = 0.0,

    //trigger msgs/TODO: remove msging system

    pub m_fMaxTriggerMessageDelay  : f32 = 0.0,
	pub m_fDelayStartTime          : f32 = 0.0,

	pub m_hstrActivationSound      : Vec<Char> = NULL,
	pub m_fSoundRadius			   : f32 = 200.0,
	pub m_bActive				   : bool = TRUE,
        //	m_bDelay				= DFALSE;

	pub m_bTouchActivate		   : bool = TRUE,
	pub m_bPlayerActivate		   : bool = TRUE,
	pub m_bAIActivate			   : bool = TRUE,
	pub m_bObjectActivate		   : bool = FALSE,
	pub m_bTriggerRelayActivate    : bool = TRUE,
	pub m_bNamedObjectActivate	   : bool = FALSE,
	pub m_hstrActivationObjectName : Vec<Char> = NULL,
	//VEC_INIT(m_vDims); ??
	//pub m_hLastSender			   : Vec<Char> = NULL,

	pub m_bLocked				   : bool = FALSE,
	pub m_hstrLockedMsg			   : Vec<Char> = NULL,
	pub m_hstrLockedSound		   : Vec<Char> = NULL,
	pub m_hstrUnlockedMsg		   : Vec<Char> = NULL,
	pub m_hstrUnlockedSound		   : Vec<Char> = NULL,
	pub m_hstrKeyName			   : Vec<Char> = NULL,

	pub m_nCurrentActivation	   : u16 = 0,
	pub m_nActivationCount		   : u16 = 1,

	pub m_bSending				   : bool = FALSE,
}