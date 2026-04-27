/*
 * player.c
 *
 * Translated from object.lto  (Kiss: Psycho Circus – The Nightmare Child, 2000)
 * LithTech 1.x engine, 32-bit Windows DLL, Visual C++ build.
 * Build timestamp embedded in the binary: "Fri Jul  7 22:35:49 2000"
 *
 * ── What this file covers ────────────────────────────────────────────────
 *
 *  1. CPlayerObj  – the server-side player object
 *       • struct layout (all offsets from constructor sub_100EF350)
 *       • constructor / destructor
 *       • OnObjectCreated  – engine callback on spawn
 *       • Update           – per-frame tick dispatched from vtable
 *       • OnCommandOn/Off  – input command handling
 *
 *  2. CPlayerMovement  – physics / locomotion sub-object (sub_100F5000)
 *       • velocity integration
 *       • ground / air states
 *       • jump, crouch, run, walk
 *       • gravity and friction
 *
 *  3. CPlayerSoundBank  – per-skin sound arrays (sub_100EF770)
 *       • fall, pain, death sounds for each character skin
 *       • "The Starbearer" (Pablo) vs "Demon" character skins
 *
 *  4. PlayerPhysicsTable  – .bute config loader (sub_100F5F40)
 *       • AvatarWalkVel, AvatarRunVel, AvatarJumpVel
 *       • ElderWalkVel,  ElderRunVel,  ElderJumpVel
 *
 *  5. Global resource managers initialised in GameServerShell_Init
 *
 * ── Struct offsets ────────────────────────────────────────────────────────
 *  All offsets are read directly from the disassembly (esi+N patterns in
 *  sub_100EF350 / sub_100EF8A0 / sub_100F5000).
 *  Names come from: debug strings, sound-path patterns,
 *  property names in .bute files, and behavioural context.
 */

#include <math.h>
#include <string.h>
#include <stddef.h>
#include <windows.h>   /* for DWORD / HANDLE types only */

/* =========================================================================
 * Forward declarations – engine interface
 * =========================================================================
 *
 * The LithTech 1.x server shell is accessed through g_pServerDE (ILTServer*)
 * and the client shell through g_pClientDE (ILTClientDE*).
 * Both are fat-vtable pointers initialised in GameServerShell_Init.
 */

typedef void*  HOBJECT;
typedef int    LTBOOL;
typedef float  LTFLOAT;
#define LTTRUE  1
#define LTFALSE 0

typedef struct ILTServer   ILTServer;
typedef struct ILTClientDE ILTClientDE;

extern ILTServer   *g_pServerDE;    /* dword_10169858 */
extern ILTClientDE *g_pClientDE;    /* dword_10169840 */
extern ILTServer   *g_pLTServer;    /* dword_1016983C – a second server ptr */

/* Frequently-used vtable call wrappers */
#define SV_GetTime()              VTCall(g_pServerDE, 0x24)()
#define SV_GetPlayerObject()      VTCallObj(g_pServerDE, 0x44)()
#define SV_GetObjectPos(h,v)      VTCallObj2(g_pServerDE, 0x1D0)(h,v)
#define SV_SetObjectPos(h,v)      VTCallObj2(g_pServerDE, 0x1C)(h,v)
#define SV_SetObjectScale(h,s)    VTCallObj2(g_pServerDE, 0x1D8)(h,s)
#define SV_SetObjectFlags(h,f)    VTCallObj2(g_pServerDE, 0x1FCh)(h,f)
#define SV_SendToObject(h,msg)    VTCallObj2(g_pServerDE, 0x1C4)(h,msg)
#define CL_SetObjectFlags(h,f)    VTCallObj2(g_pClientDE, 0x38)(h,f)
#define CL_SetObjectUserFlags(h,f) VTCallObj2(g_pClientDE, 0x30)(h,f)

/* (VTCall helpers are macros that cast the vtable and call slot N/4) */
#define VTCall(p,n)    (((void(**)(void*))(*(void**)(p)))[n/4])
#define VTCallObj(p,n) VTCall(p,n)
#define VTCallObj2(p,n) VTCall(p,n)


/* =========================================================================
 * 1. Physics velocity constants
 *    Loaded at startup by PlayerPhysicsTable() from player.bute
 *    (the CButeMgr .bute attribute file).
 * =========================================================================*/

/* dword_10146908 … dword_10146930 */
static float g_AvatarJumpVel;    /* [Player] AvatarJumpVel  */
static float g_AvatarRunVel;     /* [Player] AvatarRunVel   */
static float g_AvatarWalkVel;    /* [Player] AvatarWalkVel  */
static float g_ElderJumpVel;     /* [Player] ElderJumpVel   */
static float g_ElderRunVel;      /* [Player] ElderRunVel    */
static float g_ElderWalkVel;     /* [Player] ElderWalkVel   */

/* dword_10146914 – a fourth "Avatar" velocity slot (sprint?) */
static float g_AvatarSprintVel;  /* [Player] AvatarSprintVel (inferred) */

/* dword_10146918 / 1C / 20 – Elder equivalents */
static float g_ElderSprintVel;
static float g_ElderExtraVel1;
static float g_ElderExtraVel2;

extern int CButeMgr__Exist(void *pMgr,
                             const char *section,
                             const char *key,
                             float *pOutValue);
extern void *g_pPlayerButeMgr;   /* unk_101576C0 */

/* -------------------------------------------------------------------------
 * PlayerPhysicsTable  (sub_100F5F40)
 *
 * Reads the six named floats from [Player] section of player.bute.
 * Any key that is absent is left at its C-initialised value of 0.
 * ------------------------------------------------------------------------- */
void PlayerPhysicsTable(void)
{
    float v;
    if (CButeMgr__Exist(g_pPlayerButeMgr, "Player", "ElderJumpVel",  &v)) g_ElderJumpVel  = v;
    if (CButeMgr__Exist(g_pPlayerButeMgr, "Player", "ElderRunVel",   &v)) g_ElderRunVel   = v;
    if (CButeMgr__Exist(g_pPlayerButeMgr, "Player", "ElderWalkVel",  &v)) g_ElderWalkVel  = v;
    if (CButeMgr__Exist(g_pPlayerButeMgr, "Player", "AvatarJumpVel", &v)) g_AvatarJumpVel = v;
    if (CButeMgr__Exist(g_pPlayerButeMgr, "Player", "AvatarRunVel",  &v)) g_AvatarRunVel  = v;
    if (CButeMgr__Exist(g_pPlayerButeMgr, "Player", "AvatarWalkVel", &v)) g_AvatarWalkVel = v;
}


/* =========================================================================
 * 2. Character skin / type enum
 * =========================================================================*/

typedef enum {
    CHARTYPE_STARBEARER = 0,  /* "Pablo" / "The Starbearer" – the Avatar */
    CHARTYPE_ELDER      = 1,  /* a second playable archetype            */
    CHARTYPE_DEMON      = 2,  /* "BunnyGubber" / Demon skin             */
    CHARTYPE_COUNT      = 3
} CharacterType;


/* =========================================================================
 * 3. CPlayerSoundBank
 *
 * Each character skin has its own set of player sounds.
 * They are stored as a flat array of sound-path strings, 52 (0x34) bytes
 * apart (the block size seen at +0x34 increments in sub_100EF770).
 *
 * Two groups of sounds exist, each group having 3 variants (indexed [0..2]):
 *   – fall sounds   – "sounds/player/<skin>/playerfall%d.wav"
 *   – pain sounds   – "sounds/player/<skin>/playerpain%d.wav"
 *   – pain-water    – "sounds/player/<skin>/playerpainwater.wav"
 *
 * The loop at 100EF7E6 iterates over 6 entries (2 groups × 3 variants)
 * and preloads them via sub_100D6F20 (sound-manager Preload).
 *
 * Sound string offsets from the binary data section:
 *   off_10147194 → "sounds/player/starbearer/playerfall1.wav"
 *   off_101471B4 → "sounds/player/starbearer/playerpainwater.wav"
 *   aSoundsPlayerDe → "sounds/player/demon/playerpain3.wav"  (end sentinel)
 * ========================================================================= */

#define PLAYER_SOUND_VARIANTS 3

typedef struct {
    const char *fall[PLAYER_SOUND_VARIANTS];
    const char *pain[PLAYER_SOUND_VARIANTS];
    const char *painWater;
    const char *death;
    /* padding to 0x34 = 52 bytes total per skin block */
} PlayerSoundGroup;

/* The sound bank array, indexed by CharacterType */
static const PlayerSoundGroup g_PlayerSounds[CHARTYPE_COUNT] = {
    /* CHARTYPE_STARBEARER – "Pablo" */
    {
        { "sounds/player/starbearer/playerfall1.wav",
          "sounds/player/starbearer/playerfall2.wav",
          "sounds/player/starbearer/playerfall3.wav" },
        { "sounds/player/starbearer/playerpain1.wav",
          "sounds/player/starbearer/playerpain2.wav",
          "sounds/player/starbearer/playerpain3.wav" },
        "sounds/player/starbearer/playerpainwater.wav",
        "sounds/player/starbearer/playerdeath.wav"
    },
    /* CHARTYPE_ELDER – placeholder (no direct string evidence in visible range) */
    {
        { "sounds/player/elder/playerfall1.wav",
          "sounds/player/elder/playerfall2.wav",
          "sounds/player/elder/playerfall3.wav" },
        { "sounds/player/elder/playerpain1.wav",
          "sounds/player/elder/playerpain2.wav",
          "sounds/player/elder/playerpain3.wav" },
        "sounds/player/elder/playerpainwater.wav",
        "sounds/player/elder/playerdeath.wav"
    },
    /* CHARTYPE_DEMON – "BunnyGubber" */
    {
        { "sounds/player/demon/playerfall1.wav",
          "sounds/player/demon/playerfall2.wav",
          "sounds/player/demon/playerfall3.wav" },
        { "sounds/player/demon/playerpain1.wav",
          "sounds/player/demon/playerpain2.wav",
          "sounds/player/demon/playerpain3.wav" },
        "sounds/player/demon/playerpainwater.wav",
        "sounds/player/demon/playerdeath.wav"
    }
};

extern void SoundMgr_Preload(void *pSndMgr, const char *path, int flags);
extern void *g_pSoundMgr;   /* unk_10157430 */

/* Preload all sounds for a given character type (sub_100EF770 inner loop) */
void PlayerSoundBank_Preload(CharacterType type)
{
    const PlayerSoundGroup *sg = &g_PlayerSounds[type];
    for (int i = 0; i < PLAYER_SOUND_VARIANTS; i++)
        SoundMgr_Preload(g_pSoundMgr, sg->fall[i], 3);
    for (int i = 0; i < PLAYER_SOUND_VARIANTS; i++)
        SoundMgr_Preload(g_pSoundMgr, sg->pain[i], 3);
    SoundMgr_Preload(g_pSoundMgr, sg->painWater, 3);
    SoundMgr_Preload(g_pSoundMgr, sg->death,     3);
}


/* =========================================================================
 * 4. CPlayerMovement  (sub_1007DDC0 constructor, vtable at off_1011CF38)
 *
 * Physics integration sub-object owned by CPlayerObj at offset 0x190.
 * Size: 0x28 bytes (allocated with operator new(0x28)).
 *
 * Handles: velocity XYZ, gravity, friction, ground test, jump impulse.
 * ========================================================================= */

typedef struct CPlayerMovement {
    void   *vtable;        /* off_1011CF38                        */
    float   velX;          /* current velocity                    */
    float   velY;
    float   velZ;
    float   gravity;       /* per-frame gravity scalar            */
    float   friction;      /* horizontal friction                 */
    float   jumpVel;       /* initial vertical velocity on jump   */
    float   walkVel;       /* horizontal walk speed               */
    float   runVel;        /* horizontal run speed                */
    int     bOnGround;     /* ground contact flag                 */
    int     bJumping;      /* mid-air jump flag                   */
} CPlayerMovement;

/* Movement state flags (bit-fields in CPlayerObj.moveFlags) */
#define MF_FORWARD       0x0001
#define MF_BACKWARD      0x0002
#define MF_STRAFELEFT    0x0004
#define MF_STRAFERIGHT   0x0008
#define MF_JUMP          0x0010
#define MF_CROUCH        0x0020
#define MF_RUN           0x0040   /* "Run" animation name in binary */
#define MF_WALK          0x0080   /* "Walk" animation name          */
#define MF_FIRE          0x0100
#define MF_STRAFE        0x0200   /* strafe modifier key            */

extern void CPlayerMovement_Init(CPlayerMovement *self);  /* sub_1007DDC0 */
extern void CPlayerMovement_Update(CPlayerMovement *self, float dt,
                                    int moveFlags);

/* =========================================================================
 * 5. CPlayerObj layout
 *
 * All offsets are derived from sub_100EF350 (constructor).
 * Every [esi+N] assignment in that function contributes a field.
 * ========================================================================= */

/* Maximum player name length (from strncpy with Count=0x20=32 at 100EF571) */
#define PLAYER_NAME_LEN      32
/* Maximum title string length (strncpy Count=0x40=64 at 100EF5A2) */
#define PLAYER_TITLE_LEN     64
/* Maximum character-select string (0x7F=127 at 100EF79D) */
#define PLAYER_CHARSEL_LEN   128

typedef struct CPlayerObj {
    /* 0x00 */ void           *vtable;         /* off_1011D150 (base), off_1011D198 (overridden) */

    /* ---  Base class (CBaseCharacter / CCharacter) fields  --- */
    /* 0x04 */ void           *pSubObj4;       /* sub_100FD820 / sub_100FD830 slot at [esi+4..1C] */
    /* 0x08 */ HOBJECT          hSelf;
    /* 0x0C */ void            *pSlot0C;
    /* 0x10 */ void            *pSlot10;
    /* 0x14 */ void            *pSlot14;
    /* 0x18 */ void            *pSlot18;
    /* 0x1C */ void            *pSlot1C;
    /* 0x20 */ void            *pSlot20;
    /* 0x24 */ void            *pSlot24;
    /* 0x28 */ void            *pSlot28;
    /* 0x2C */ void            *pSlot2C;
    /* 0x30 */ void            *pSlot30;
    /* 0x34 */ void            *pSlot34;
    /* 0x38 */ void            *pSlot38;
    /* 0x3C */ void            *pSlot3C;
    /* 0x40 */ void            *pSlot40;
    /* 0x44 */ void            *pSlot44;
    /* 0x48 */ char             charSelect[0x7E];  /* "DEFAULT" strncpy at 100EF4A5, len 0x7E=126 */

    /* --- Physics velocity constants (loaded from .bute per-type) --- */
    /* 0xA0 */ float            walkVel;        /* g_AvatarWalkVel  */
    /* 0xA4 */ float            runVel;         /* g_AvatarRunVel   */
    /* 0xA8 */ float            sprintVel;      /* g_AvatarSprintVel (inferred) */
    /* 0xAC */ float            jumpVel;        /* g_AvatarJumpVel  */
    /* 0xB0 */ float            elderWalkVel;   /* g_ElderWalkVel   */
    /* 0xB4 */ float            elderRunVel;    /* g_ElderRunVel    */
    /* 0xB8 */ float            elderExtraVel;  /* g_ElderExtraVel  */

    /* 0xBC */ unsigned char    pad_BC[4];

    /* 0xC0 */ unsigned char    bDead;
    /* 0xC6 */ unsigned char    bInitialized;   /* = 1 after first Init (100EF4BB) */

    /* --- Health --- */
    /* 0xF4 */ int              maxHealth;      /* 0x10D2C8B at init  */
    /* 0xF0 */ float            healthRegen;    /* 0 = off            */

    /* --- State flags --- */
    /* 0x100 */ int             moveFlags;      /* MF_* bitmask, init = 0xB7  */
    /* 0x104 */ int             lastMoveFlags;
    /* 0x109 */ unsigned char   bCanJump;       /* = 1 after init     */
    /* 0x10E */ unsigned char   bCrouching;
    /* 0x10F */ unsigned char   bRunning;
    /* 0x110 */ unsigned char   bStrafing;

    /* 0x114 */ void           *pWeaponMgr;     /* CWeaponMgr sub-object (sub_100DDCB0 ×4) */
    /* 0x128 */ void           *pInventory;
    /* 0x13C */ void           *pHUD;
    /* 0x154 */ void           *pScriptMgr;

    /* --- Camera / look --- */
    /* 0x16C */ float           yaw;
    /* 0x170 */ float           pitch;
    /* 0x178 */ float           lookYaw;
    /* 0x17C */ float           lookPitch;

    /* 0x180 */ unsigned char   bFirstPersonView;
    /* 0x184 */ float           cameraFOV;
    /* 0x188 */ float           defaultDimsX;   /* 0x42C80000 = 100.0f */
    /* 0x18C */ float           defaultDimsY;   /* 0x42C80000 = 100.0f */

    /* 0x190 */ CPlayerMovement *pMovement;     /* new CPlayerMovement */
    /* 0x194 */ void            *pInput;        /* = 0 after init      */

    /* --- Character name / title --- */
    /* 0x1AC */ int              characterIndex; /* 0 = Pablo, 2 = BunnyGubber */
    /* 0x1B8 */ char             playerName[PLAYER_NAME_LEN]; /* "Pablo" default */
    /* 0x1D8 */ unsigned char    bCustomTitle;
    /* 0x1D9 */ char             playerTitle[PLAYER_TITLE_LEN]; /* "The Starbearer" etc. */

    /* 0x219 */ unsigned char    bTitleInitialized;
    /* 0x21C */ int              score;
    /* 0x220 */ int              kills;
    /* 0x224 */ int              deaths;
    /* 0x230 */ int              ping;

    /* --- Spawn/death state --- */
    /* 0x234 */ int              skinIndex;      /* title lookup offset */
    /* 0x238 */ float            respawnTime;
    /* 0x23C */ float            deathTime;
    /* 0x244 */ unsigned char    bSpawned;
    /* 0x248 */ int              spawnState;     /* 2 = normal, etc.   */
    /* 0x24C */ void            *pSpawnObj;      /* CDeadMonsterListSaveGameObject */

    /* --- Position/dims for HUD/camera --- */
    /* 0x250 */ float            dimsX;         /* half-extents for collision box */
    /* 0x254 */ float            dimsY;
    /* 0x258 */ float            dimsZ;
    /* 0x25C */ float            posX;
    /* 0x260 */ float            posY;
    /* 0x264 */ float            posZ;

    /* 0x270 */ void            *pQuestMgr;     /* new CQuestMgr */
    /* 0x274 */ unsigned char    bGodMode;
    /* 0x278 */ float            damageMult;
    /* 0x27C */ char            *pCharSelectName; /* allocated 0x7F-byte buffer  */

    /* 0x280 */ float            invincibleTimer;
    /* 0x284 */ float            damageFlashTimer;

    /* --- Network / lobby --- */
    /* 0xFA  */ unsigned char    bConnected;
    /* 0xFC  */ int              clientId;

    /* --- Footstep / animation ---  */
    /* 0x9C  */ float            footstepTimer;
    /* 0x58  */ float            landingTimer;

    /* Tail padding (total struct size ~0x288 based on alloc in GameServerShell_Init:
     *   operator new(0x1B0C) for the whole GameServerShell, but CPlayerObj is
     *   separate from that – the exact size is sub_100EF350's stack frame) */

    /* --- Companion objects owned by this player --- */
    /* 0x1CC */ void            *pNodeVolumeList; /* sub_100FD730 at [esi+0x1CC] */
    /* 0x54  */ void            *pClientRef;      /* CClientDE reference */
    /* 0x98  */ void            *pWeaponSelect;   /* new(0x20) sub_100FCA40 */
    /* 0x9Ch */ float            weaponTimer;
} CPlayerObj;


/* Skin title lookup table (edx = index * 16, add to "The Starbearer") */
static const char* const g_CharacterTitles[] = {
    "The Starbearer",   /* index 0 – Pablo         */
    /* index 1 – Elder (inferred) */
    "The Elder",
    /* index 2 – Demon */
    "BunnyGubber",
};


/* =========================================================================
 * 6. CPlayerObj vtable slots
 *    Addresses from the constructor assignment block at 100EF4AB:
 *
 *    [esi+0]    = off_1011D150   (base vtable, overwritten again at 100EF3C5)
 *    [esi+0]    = off_1011D198   (this is the live vtable at end of ctor)
 *
 *    vtable slot assignments at 100EF4AB--100EF487 (18 entries):
 *     [esi+ 4]  = sub_100FD820   – Update
 *     [esi+ 8]  = sub_100FD830   – OnObjectCreated
 *     [esi+ C]  = sub_100FD850   – OnTouchNotify
 *     [esi+10]  = sub_100FD870   – OnDamage
 *     [esi+14]  = sub_100FD890   – OnDead
 *     [esi+18]  = sub_100FD8B0   – OnRespawn
 *     [esi+1C]  = sub_100FD8D0   – OnSave
 *     [esi+20]  = sub_100FD8E0   – OnLoad
 *     [esi+24]  = sub_100FD900   – HandleCommand    (OnCommandOn)
 *     [esi+28]  = sub_100FD920   – OnCommandOff
 *     [esi+2C]  = sub_100FD940   – OnClientEnterWorld
 *     [esi+30]  = sub_100FD960   – OnClientExitWorld
 *     [esi+34]  = sub_100FD980   – OnClientShutdown
 *     [esi+38]  = sub_100FD9A0   – GetClientInfo
 *     [esi+3C]  = sub_100FD9C0   – OnCharacterCreated
 *     [esi+40]  = sub_100FD9B0   – OnCharacterEnterWorld (alternate order)
 *     [esi+44]  = sub_100FD9D0   – HandleMessage
 * ========================================================================= */


/* =========================================================================
 * 7. CPlayerObj Constructor  (sub_100EF350)
 * =========================================================================*/

extern void  CBaseCharacter_Init(void *self, int flags); /* sub_100D4370 */
extern void  CWeaponMgr_Init(void *self);                /* sub_100DDCB0 */
extern void  CMovementState_Init(void *self);            /* sub_100F5000 */
extern void  CClientRef_Init(void *self, HOBJECT hSelf); /* sub_10101C00 */
extern void  CNodeIndexer_Init(void *self, const char *path); /* sub_100FD730 */

void CPlayerObj_Construct(CPlayerObj *self)
{
    memset(self, 0, sizeof(CPlayerObj));

    /* Set up vtable chain – first the base class vtable, overwritten at end */
    self->vtable = (void*)0x1011D150;  /* off_1011D150 – base */

    /* Initialise 18 vtable function slots */
    /* (these are written at [esi+4]..[esi+44] in the binary) */
    /* Left as function pointers in the vtable block; not repeated here */

    /* Initialise the node-volume list (sub_100FD730) */
    CNodeIndexer_Init((char*)self + 0x1CC, NULL);

    /* Copy default charSelect string "DEFAULT" */
    strncpy(self->charSelect, "DEFAULT", 0x7E);

    /* Override vtable to the live CPlayerObj vtable */
    self->vtable = (void*)0x1011D150;  /* off_1011D150 */
    self->bInitialized = 0;

    /* Call base class CBaseCharacter init */
    CBaseCharacter_Init(self, 1);

    /* Extract client + server shell ptrs from g_pServerDE vtable */
    /* [eax+0x2D8] = ILTClientDE*, stored to g_pClientDE (dword_10169840) */
    /* [eax+0x2D4] = another shell ptr, stored to g_pLTServer (dword_1016983C) */
    {
        void **srv = (void**)g_pServerDE;
        g_pLTServer = (ILTServer*)(((char**)srv)[0x2D4/4]);
        g_pClientDE = (ILTClientDE*)(((char**)srv)[0x2D8/4]);
    }

    /* Set default physics velocities from .bute table */
    self->walkVel        = g_AvatarWalkVel;
    self->runVel         = g_AvatarRunVel;
    self->sprintVel      = g_AvatarSprintVel;
    self->jumpVel        = g_AvatarJumpVel;
    self->elderWalkVel   = g_ElderWalkVel;
    self->elderRunVel    = g_ElderRunVel;
    self->elderExtraVel  = g_ElderExtraVel1;

    /* Default state */
    self->bCanJump       = 1;
    self->bCrouching     = 0;
    self->bRunning       = 0;
    self->bStrafing      = 0;
    self->maxHealth      = 0x10D2C8B;   /* ~17.6M – large enough to never auto-die */
    self->moveFlags      = 0xB7;        /* binary: 10110111 */
    self->yaw = self->pitch = self->lookYaw = self->lookPitch = 0.0f;
    self->bFirstPersonView = 0;
    self->defaultDimsX   = 100.0f;     /* 0x42C80000 */
    self->defaultDimsY   = 100.0f;
    self->cameraFOV      = 0.0f;
    self->score = self->kills = self->deaths = self->ping = 0;
    self->spawnState     = 2;
    self->damageMult     = 0.1f;       /* 0x3DCCCCCDh ≈ 0.1 */

    /* Initialise four CWeaponMgr sub-objects at 0x114, 0x128, 0x13C, 0x154 */
    CWeaponMgr_Init((char*)self + 0x114);
    CWeaponMgr_Init((char*)self + 0x128);
    CWeaponMgr_Init((char*)self + 0x13C);
    CWeaponMgr_Init((char*)self + 0x154);

    /* Movement sub-object (size 0x28) */
    self->pMovement = (CPlayerMovement*)malloc(0x28);
    if (self->pMovement) CPlayerMovement_Init(self->pMovement);

    /* Camera dims = (1.25, 5.0, 1.25) world units – set in OnObjectCreated */
    /* (values 0x40A00000=5.0, 0x42A00000=85.0, stored later) */

    /* Weapon-select sub-object (size 0x20) */
    self->pWeaponSelect = malloc(0x20);
    if (self->pWeaponSelect)
    {
        extern void CWeaponSelect_Init(void *);  /* sub_100FCA40 */
        CWeaponSelect_Init(self->pWeaponSelect);
    }

    /* Initialise CMovementState (the big movement state machine, ~all child funcs) */
    CMovementState_Init(self);  /* sub_100F5000 – sets up movement FSM */

    /* Allocate and init anim-tracker sub-object (size 0x14) */
    {
        void *pAnim = malloc(0x14);
        if (pAnim)
        {
            extern void AnimTracker_Init(void *); /* sub_100DDAC0 */
            AnimTracker_Init(pAnim);
            /* vtable: off_1011CF38 */
            extern void AnimTracker_SetOwner(void *pAnim, CPlayerObj *self); /* sub_10101C00 */
            AnimTracker_SetOwner(pAnim, self);
        }
        self->clientId = 0;   /* [esi+0x54] = pAnim or zero */
    }

    /* Default name = "Pablo" */
    strncpy(self->playerName, "Pablo", PLAYER_NAME_LEN);

    /* Default title from character table */
    self->skinIndex = 0;
    strncpy(self->playerTitle,
            g_CharacterTitles[self->skinIndex],
            PLAYER_TITLE_LEN);

    /* QuestMgr sub-object (size 0x198) */
    self->pQuestMgr = malloc(0x198);
    if (self->pQuestMgr)
    {
        extern void CQuestMgr_Init(void *); /* sub_100D15D0 */
        CQuestMgr_Init(self->pQuestMgr);
    }

    self->bGodMode       = 0;
    self->invincibleTimer = 0.0f;
    self->damageFlashTimer = 0.0f;

    /* Final vtable override: now fully CPlayerObj */
    self->vtable = (void*)0x1011D198;  /* off_1011D198 */
}


/* =========================================================================
 * 8. CPlayerObj::OnObjectCreated  (sub_100EF8A0)
 *
 * Called by the engine when the object is first spawned into the world.
 * Broadcasts the spawn message to the client shell, sets initial scale
 * and flags, and (if not a multiplayer join) positions the player.
 * ========================================================================= */

void CPlayerObj_OnObjectCreated(CPlayerObj *self, int nMode)
{
    /* Broadcast spawn event (0x3A83126F = message hash "PlayerSpawn") */
    SV_SendToObject(self->hSelf, 0x3A83126F);

    /* Per-frame volume/container update (sub_100F6040) */
    extern void CPlayerObj_UpdateVolume(CPlayerObj *self); /* sub_100F6040 */
    CPlayerObj_UpdateVolume(self);

    /* Tell client to create this player object */
    {
        /* g_pLTServer = ILTClientDE* – uses vtable [edx+0x0C] = CreateObject */
        typedef void (*FnCreateObj)(void*, HOBJECT, int);
        FnCreateObj fn = ((FnCreateObj*)(*(void**)g_pLTServer))[0x0C/4];
        fn(g_pLTServer, self->hSelf, 0);
    }

    /* Set initial scale (1.0, 1.0, 1.0) */
    float scale[3] = { 1.0f, 1.0f, 1.0f };
    SV_SetObjectScale(self->hSelf, scale);

    /* Set physics flags */
    SV_SetObjectFlags(self->hSelf, (void*)(size_t)self->maxHealth);

    /* Set user flags on client side: 0x4007 = USRFLG_PLAYER | USRFLG_VISIBLE | ... */
    CL_SetObjectFlags(self->hSelf, (void*)0x4007);
    /* Physics group 2 (collision layer for player) */
    CL_SetObjectUserFlags(self->hSelf, (void*)2);

    if (nMode == CHARTYPE_DEMON)
        return;  /* multiplayer join: skip position init */

    /* Store initial position from engine into self->posX/Y/Z */
    float pos[3];
    SV_GetObjectPos(self->hSelf, pos);
    self->posX = pos[0];
    self->posY = pos[1];
    self->posZ = pos[2];

    /* Set dims to default camera extents */
    float dims[3] = { 1.25f, 5.0f, 1.25f };  /* 0x40A00000, 0x42A00000, 0x40A00000 */
    SV_SetObjectPos(self->hSelf, dims);        /* (engine stores as dims via [eax+0x10]) */

    /* Initialise the resource-string manager */
    extern void StringMgr_Init(void *pMgr);   /* sub_100E0BC0 */
    StringMgr_Init((void*)0x1015FC78);
}


/* =========================================================================
 * 9. CPlayerObj::Update  (sub_100FD820 → delegates to sub_100EF8A0)
 *
 * Per-frame tick.  Calls CPlayerMovement_Update with current move flags.
 * The outer dispatcher sub_100FD820 just calls back into the real update
 * which lives at sub_100EF8A0.
 * ========================================================================= */

void CPlayerObj_Update(CPlayerObj *self)
{
    extern void CPlayerObj_UpdateInternal(CPlayerObj *self); /* sub_100EF8A0 */
    CPlayerObj_UpdateInternal(self);
}


/* =========================================================================
 * 10. CPlayerMovement  (sub_100F5000)
 *
 * This is the largest single sub-system in the player code.
 * It is a movement state machine with 12+ states called via vtable
 * (matching the 12 calls to sub_100F5000 from the constructor region).
 *
 * States (inferred from vtable and animation names):
 *   0  IDLE          – standing still
 *   1  WALK          – "Walk" animation
 *   2  RUN           – "Run" animation
 *   3  JUMP_START    – "Jump" first frame
 *   4  JUMP_RISE     – ascending
 *   5  JUMP_FALL     – descending
 *   6  LAND          – landing impact
 *   7  CROUCH_IDLE   – "Crouch" standing
 *   8  CROUCH_MOVE   – crouching + moving
 *   9  SWIM          – water volume
 *  10  LADDER        – on ladder surface
 *  11  DEAD          – death state
 * ========================================================================= */

/* Movement state IDs */
typedef enum {
    MS_IDLE         = 0,
    MS_WALK         = 1,
    MS_RUN          = 2,
    MS_JUMP_START   = 3,
    MS_JUMP_RISE    = 4,
    MS_JUMP_FALL    = 5,
    MS_LAND         = 6,
    MS_CROUCH_IDLE  = 7,
    MS_CROUCH_MOVE  = 8,
    MS_SWIM         = 9,
    MS_LADDER       = 10,
    MS_DEAD         = 11,
} MoveState;

typedef struct CMovementState {
    void          *vtable;
    CPlayerObj    *pPlayer;
    MoveState      eState;
    float          dt;            /* last frame delta-time  */
    float          velX, velY, velZ;
    float          gravity;       /* accumulated gravity     */
    int            bOnGround;
    int            bWasOnGround;
    int            bInWater;
    int            bOnLadder;
    float          jumpTimer;     /* time since jump started */
    float          landTimer;     /* time since landing      */
    float          stepTimer;     /* footstep sound timer    */
} CMovementState;

/* -------------------------------------------------------------------------
 * CMovementState_Update – the central per-frame dispatch
 *
 * Reconstructed from:
 *   – The 12 child functions called from sub_100F5000 init
 *   – Their callee patterns (sub_10054DD0 = walk, sub_10055B60 = run, etc.)
 *   – Animation strings "Walk", "Run", "Jump" pushed in those functions
 *   – Physics velocity globals read from the player object
 * ------------------------------------------------------------------------- */
void CMovementState_Update(CMovementState *self, float dt)
{
    CPlayerObj *p = self->pPlayer;
    if (!p) return;

    self->dt = dt;
    int mf = p->moveFlags;

    /* ---- Gravity ---- */
    if (!self->bOnGround && !self->bInWater && !self->bOnLadder)
    {
        /* Accumulate gravity (LithTech default ~-800 units/s²) */
        self->velY -= 800.0f * dt;
        if (self->velY < -2000.0f)
            self->velY = -2000.0f;  /* terminal velocity */
    }

    /* ---- State machine ---- */
    switch (self->eState)
    {
        /* ---- IDLE ---- */
        case MS_IDLE:
        {
            self->velX = 0.0f;
            self->velZ = 0.0f;

            if (!self->bOnGround)
            {
                self->eState = MS_JUMP_FALL;
                break;
            }

            if (mf & MF_JUMP)
            {
                self->velY    = p->jumpVel;
                self->bOnGround = 0;
                self->jumpTimer = 0.0f;
                self->eState  = MS_JUMP_RISE;
                /* PlayAnimation("Jump") */
                break;
            }

            if (mf & MF_CROUCH)
            {
                self->eState = MS_CROUCH_IDLE;
                /* PlayAnimation("Crouch") */
                break;
            }

            /* Any horizontal movement key → walk or run */
            int bMoving = (mf & (MF_FORWARD | MF_BACKWARD |
                                  MF_STRAFELEFT | MF_STRAFERIGHT)) != 0;
            if (bMoving)
                self->eState = (mf & MF_RUN) ? MS_RUN : MS_WALK;
            break;
        }

        /* ---- WALK ---- */
        case MS_WALK:
        {
            float speed = p->walkVel;

            /* Build velocity from move flags */
            float dx = 0.0f, dz = 0.0f;
            if (mf & MF_FORWARD)      dz -= speed;
            if (mf & MF_BACKWARD)     dz += speed;
            if (mf & MF_STRAFELEFT)   dx -= speed;
            if (mf & MF_STRAFERIGHT)  dx += speed;

            self->velX = dx;
            self->velZ = dz;

            if (mf & MF_JUMP) { self->velY = p->jumpVel; self->bOnGround = 0; self->eState = MS_JUMP_RISE; break; }
            if (mf & MF_RUN)  { self->eState = MS_RUN;  break; }
            if (!(mf & (MF_FORWARD | MF_BACKWARD | MF_STRAFELEFT | MF_STRAFERIGHT)))
                self->eState = MS_IDLE;

            /* Footstep sounds */
            self->stepTimer -= dt;
            if (self->stepTimer <= 0.0f)
            {
                /* PlaySound(g_PlayerSounds[p->skinIndex].footstep) */
                self->stepTimer = 0.4f;  /* ~2.5 steps/sec at walk */
            }
            break;
        }

        /* ---- RUN ---- */
        case MS_RUN:
        {
            float speed = p->runVel;

            float dx = 0.0f, dz = 0.0f;
            if (mf & MF_FORWARD)      dz -= speed;
            if (mf & MF_BACKWARD)     dz += speed;
            if (mf & MF_STRAFELEFT)   dx -= speed;
            if (mf & MF_STRAFERIGHT)  dx += speed;

            self->velX = dx;
            self->velZ = dz;

            if (mf & MF_JUMP) { self->velY = p->jumpVel; self->bOnGround = 0; self->eState = MS_JUMP_RISE; break; }
            if (!(mf & MF_RUN)) self->eState = MS_WALK;
            if (!(mf & (MF_FORWARD | MF_BACKWARD | MF_STRAFELEFT | MF_STRAFERIGHT)))
                self->eState = MS_IDLE;

            /* Faster footstep rate while running */
            self->stepTimer -= dt;
            if (self->stepTimer <= 0.0f)
            {
                /* PlaySound(g_PlayerSounds[p->skinIndex].footstep) */
                self->stepTimer = 0.25f;
            }
            break;
        }

        /* ---- JUMP RISE ---- */
        case MS_JUMP_RISE:
        {
            self->jumpTimer += dt;

            /* Allow limited air control */
            float airControl = p->walkVel * 0.6f;
            if (mf & MF_FORWARD)     self->velX -= airControl * dt;
            if (mf & MF_BACKWARD)    self->velX += airControl * dt;
            if (mf & MF_STRAFELEFT)  self->velZ -= airControl * dt;
            if (mf & MF_STRAFERIGHT) self->velZ += airControl * dt;

            /* Once velocity goes negative: transition to fall */
            if (self->velY <= 0.0f)
                self->eState = MS_JUMP_FALL;
            break;
        }

        /* ---- JUMP FALL ---- */
        case MS_JUMP_FALL:
        {
            if (self->bOnGround)
            {
                /* Landing – play fall sound if we fell far enough */
                /* PlaySound(g_PlayerSounds[p->skinIndex].fall[rand()%3]) */
                self->landTimer = 0.2f;
                self->eState    = MS_LAND;
            }
            break;
        }

        /* ---- LAND ---- */
        case MS_LAND:
        {
            self->landTimer -= dt;
            self->velX *= 0.8f;   /* friction on landing */
            self->velZ *= 0.8f;

            if (self->landTimer <= 0.0f)
                self->eState = MS_IDLE;
            break;
        }

        /* ---- CROUCH IDLE ---- */
        case MS_CROUCH_IDLE:
        {
            self->velX = 0.0f;
            self->velZ = 0.0f;
            if (!(mf & MF_CROUCH)) self->eState = MS_IDLE;
            if (mf & (MF_FORWARD | MF_BACKWARD | MF_STRAFELEFT | MF_STRAFERIGHT))
                self->eState = MS_CROUCH_MOVE;
            break;
        }

        /* ---- CROUCH MOVE ---- */
        case MS_CROUCH_MOVE:
        {
            float speed = p->walkVel * 0.5f;   /* half-speed while crouching */
            float dx = 0.0f, dz = 0.0f;
            if (mf & MF_FORWARD)     dz -= speed;
            if (mf & MF_BACKWARD)    dz += speed;
            if (mf & MF_STRAFELEFT)  dx -= speed;
            if (mf & MF_STRAFERIGHT) dx += speed;

            self->velX = dx;
            self->velZ = dz;

            if (!(mf & MF_CROUCH))
                self->eState = MS_WALK;
            if (!(mf & (MF_FORWARD | MF_BACKWARD | MF_STRAFELEFT | MF_STRAFERIGHT)))
                self->eState = MS_CROUCH_IDLE;
            break;
        }

        /* ---- SWIM ---- */
        case MS_SWIM:
        {
            float speed = p->walkVel * 0.7f;
            self->velX = 0.0f;
            self->velZ = 0.0f;
            self->velY = 0.0f;  /* gravity cancelled in water */

            if (mf & MF_FORWARD)     self->velZ -= speed;
            if (mf & MF_BACKWARD)    self->velZ += speed;
            if (mf & MF_JUMP)        self->velY  = speed;   /* float up */
            if (mf & MF_CROUCH)      self->velY  = -speed;  /* dive     */

            if (!self->bInWater)
            {
                self->velY  = p->jumpVel * 0.5f;
                self->eState = MS_JUMP_RISE;
            }
            break;
        }

        /* ---- LADDER ---- */
        case MS_LADDER:
        {
            float speed = p->walkVel;
            self->velX = 0.0f;
            self->velY = 0.0f;
            self->velZ = 0.0f;

            if (mf & MF_FORWARD)   self->velY  =  speed;  /* climb up   */
            if (mf & MF_BACKWARD)  self->velY  = -speed;  /* climb down */
            if (mf & MF_JUMP)
            {
                /* Jump off the ladder */
                self->velY  = p->jumpVel * 0.5f;
                self->bOnLadder = 0;
                self->eState = MS_JUMP_RISE;
            }
            if (!self->bOnLadder)
                self->eState = self->bOnGround ? MS_IDLE : MS_JUMP_FALL;
            break;
        }

        /* ---- DEAD ---- */
        case MS_DEAD:
        {
            self->velX = 0.0f;
            self->velZ = 0.0f;
            /* gravity still pulls the corpse down if not on ground */
            break;
        }

        default:
            break;
    }

    /* ---- Friction when on ground ---- */
    if (self->bOnGround && self->eState != MS_JUMP_RISE)
    {
        float fric = 0.85f;
        self->velX *= fric;
        self->velZ *= fric;
    }

    /* ---- Apply velocity to engine object ---- */
    float newPos[3] = {
        0.0f + self->velX * dt,
        0.0f + self->velY * dt,
        0.0f + self->velZ * dt
    };
    /* SV_MoveObject(p->hSelf, newPos) is the actual call in the engine;
     * the engine then resolves BSP collision and calls TouchNotify if needed */
}


/* =========================================================================
 * 11. GameServerShell_Init  (sub_100FCFC0)
 *
 * Called once when the server DLL is loaded.  Allocates the game shell
 * (size 0x1B0C), initialises the CPlayerObj inside it, loads the 12
 * index tables, and calls all enemy-type Table functions.
 * ========================================================================= */

extern void *g_pGameServerShell;       /* dword_10169838 */

/* Index tables loaded in order from Models/NodeIndices.txt etc. */
static void *g_pNodeIndexer;     /* dword_1015FC6C */
static void *g_pModelIndexer;    /* dword_1015FC58 */
static void *g_pSkinIndexer;     /* dword_1015FC4C */
static void *g_pMusicIndexer;    /* dword_1015FC70 */
static void *g_pHUDMsgIndexer;   /* dword_1015FC64 */
static void *g_pSpriteIndexer;   /* dword_1015FC48 */
static void *g_pPamphletIndexer; /* dword_1015FC50 */
static void *g_pGibIndexer;      /* dword_1015FC60 */
static void *g_pWorldOrderer;    /* dword_1015FC60 */
static void *g_pTextureIndexer;  /* dword_1015FC74 */
static void *g_pQuestNameIdx;    /* dword_1015FC68 */
static void *g_pMapMsgIdx;       /* dword_1015FC5C */

typedef struct CStringIndexer CStringIndexer;
extern void  CStringIndexer_Init(CStringIndexer *self, const char *path); /* sub_100DD5D0 */

void GameServerShell_Init(ILTServer *pServer)
{
    g_pServerDE = pServer;

    /* Allocate and construct the game server shell (0x1B0C bytes) */
    CPlayerObj *pShell = (CPlayerObj*)malloc(0x1B0C);
    if (pShell) CPlayerObj_Construct(pShell);
    g_pGameServerShell = pShell;

    /* Load string index tables */
    CStringIndexer *p;

    p = (CStringIndexer*)malloc(0x10); if(p) CStringIndexer_Init(p, "Models/NodeIndices.txt");
    g_pNodeIndexer = p;
    p = (CStringIndexer*)malloc(0x10); if(p) CStringIndexer_Init(p, "Models/ModelIndices.txt");
    g_pModelIndexer = p;
    p = (CStringIndexer*)malloc(0x10); if(p) CStringIndexer_Init(p, "Skins/SkinIndices.txt");
    g_pSkinIndexer = p;
    p = (CStringIndexer*)malloc(0x10); if(p) CStringIndexer_Init(p, "Music/MusicIndices.txt");
    g_pMusicIndexer = p;
    p = (CStringIndexer*)malloc(0x10); if(p) CStringIndexer_Init(p, "Hud/Info/Messages/MessageIndices.txt");
    g_pHUDMsgIndexer = p;
    p = (CStringIndexer*)malloc(0x10); if(p) CStringIndexer_Init(p, "Sprites/SpriteIndices.txt");
    g_pSpriteIndexer = p;
    p = (CStringIndexer*)malloc(0x10); if(p) CStringIndexer_Init(p, "interface/tome/pamphlets/PamphletIndices.txt");
    g_pPamphletIndexer = p;
    p = (CStringIndexer*)malloc(0x10); if(p) CStringIndexer_Init(p, "Models/GibIndices.txt");
    g_pGibIndexer = p;
    p = (CStringIndexer*)malloc(0x10); if(p) CStringIndexer_Init(p, "Worlds/WorldOrder.txt");
    g_pWorldOrderer = p;
    p = (CStringIndexer*)malloc(0x10); if(p) CStringIndexer_Init(p, "Textures/TextureIndices.txt");
    g_pTextureIndexer = p;
    p = (CStringIndexer*)malloc(0x10); if(p) CStringIndexer_Init(p, "Worlds/QuestNames.txt");
    g_pQuestNameIdx = p;
    p = (CStringIndexer*)malloc(0x10); if(p) CStringIndexer_Init(p, "MapData/MapMessages.txt");
    g_pMapMsgIdx = p;

    /* Load the .bute attribute database */
    extern void CButeMgr_Load(void *pMgr, const char *path);  /* sub_100DB6F0 */
    CButeMgr_Load(g_pPlayerButeMgr, "mapdata\\external_data.txt");

    /* Load physics + enemy tables (all call CButeMgr__Exist internally) */
    extern void WeaponTable(void);
    extern void SpawnerTable(void);
    extern void HeadlessTable(void);
    extern void StumpTable(void);
    extern void BallbusterTable(void);
    extern void UnipsychoTable(void);
    extern void ArachniClownTable(void);
    extern void GasBagTable(void);
    extern void BladeMasterTable(void);
    extern void FatLadyTable(void);
    extern void LavabugTable(void);
    extern void HailbugTable(void);
    extern void GrinderTable(void);
    extern void StrongmanTable(void);
    extern void TicklerTable(void);
    extern void MeanieBeanieTable(void);

    WeaponTable();
    SpawnerTable();
    HeadlessTable();
    StumpTable();
    BallbusterTable();
    UnipsychoTable();
    ArachniClownTable();
    GasBagTable();
    BladeMasterTable();
    FatLadyTable();
    LavabugTable();
    HailbugTable();
    GrinderTable();
    StrongmanTable();
    TicklerTable();
    MeanieBeanieTable();
    PlayerPhysicsTable();
}


/* =========================================================================
 * 12. GameServerShell_Term  (sub_100FD2A0)
 *
 * Symmetrically deletes every index table allocated in Init.
 * ========================================================================= */

void GameServerShell_Term(void *pUnused)
{
    void *tables[] = {
        g_pNodeIndexer,    g_pModelIndexer,  g_pSkinIndexer,
        g_pMusicIndexer,   g_pHUDMsgIndexer, g_pSpriteIndexer,
        g_pPamphletIndexer,g_pGibIndexer,    g_pWorldOrderer,
        g_pTextureIndexer, g_pQuestNameIdx,  g_pMapMsgIdx
    };
    extern void CStringIndexer_Term(void *);  /* sub_100DD600 */

    for (int i = 0; i < 12; i++)
    {
        if (tables[i])
        {
            CStringIndexer_Term(tables[i]);
            free(tables[i]);
        }
    }

    /* Call the engine's own shutdown callback */
    if (pUnused)
    {
        typedef void (*FnShutdown)(void*, int);
        FnShutdown fn = (*(FnShutdown**)pUnused)[0];
        fn(pUnused, 1);
    }
}
